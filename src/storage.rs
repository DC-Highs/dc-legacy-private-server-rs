//! Persistence of player state on top of a `PlayerStore`, plus the per-player locks.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::Mutex;
use serde_json::{json, Map, Value};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use crate::db::{PendingAssist, PlayerStore};
use crate::error::StoreResult;
use crate::gamedata::{GameData, PlayerConfig};
use crate::json::{ensure_array, ensure_object, int_or};
use crate::util::now_ts;

pub const MAX_RECEIVED_ASSISTS: usize = 200;
pub const MAX_PLAYER_NAME_LENGTH: usize = 24;
const ASSIST_BATCH: usize = 100;

pub struct Loaded {
    pub data: Value,
    pub config: PlayerConfig,
    pub merged_assists: bool,
}

pub struct PlayerGuard {
    locks: Arc<DashMap<i64, Arc<AsyncMutex<()>>>>,
    id: i64,
    guard: Option<OwnedMutexGuard<()>>,
}

impl Drop for PlayerGuard {
    fn drop(&mut self) {
        self.guard.take();
        self.locks.remove_if(&self.id, |_, lock| Arc::strong_count(lock) == 1);
    }
}

pub struct Storage {
    store: Arc<dyn PlayerStore>,
    pub game: GameData,
    locks: Arc<DashMap<i64, Arc<AsyncMutex<()>>>>,
    pending_targets: Mutex<HashSet<i64>>,
}

impl Storage {
    pub async fn new(store: Arc<dyn PlayerStore>, game: GameData) -> StoreResult<Storage> {
        let pending = store.pending_targets().await?.into_iter().collect();
        Ok(Storage { store, game, locks: Arc::new(DashMap::new()), pending_targets: Mutex::new(pending) })
    }

    pub fn store(&self) -> &Arc<dyn PlayerStore> {
        &self.store
    }

    pub async fn lock(&self, player_id: i64) -> PlayerGuard {
        let lock = self.locks.entry(player_id).or_insert_with(|| Arc::new(AsyncMutex::new(()))).clone();
        let guard = lock.lock_owned().await;
        PlayerGuard { locks: self.locks.clone(), id: player_id, guard: Some(guard) }
    }

    pub fn build_player(&self, player_id: i64, name: &str) -> Value {
        let mut data = self.game.player_template.clone();
        normalize_player(&mut data);
        if let Some(info) = data.get_mut("playerInfo").and_then(Value::as_object_mut) {
            info.insert("name".into(), Value::from(name));
            info.insert("pid".into(), Value::from(player_id.to_string()));
        }
        if let Some(root) = data.as_object_mut() {
            root.insert("timestamp".into(), Value::from(now_ts()));
        }
        data
    }

    pub async fn create_player(&self, player_id: i64, name: &str, replace: bool) -> StoreResult<Value> {
        let data = self.build_player(player_id, name);
        self.store.create(player_id, &data, replace).await?;
        Ok(data)
    }

    pub async fn player_exists(&self, player_id: i64) -> StoreResult<bool> {
        self.store.exists(player_id).await
    }

    pub async fn delete_player(&self, player_id: i64) -> StoreResult<()> {
        self.pending_targets.lock().remove(&player_id);
        self.store.delete(player_id).await
    }

    pub async fn config_for_player(&self, player_id: i64) -> StoreResult<PlayerConfig> {
        Ok(self.game.player_config(self.store.get_kompu_offers(player_id).await?))
    }

    pub async fn peek_player(&self, player_id: i64) -> StoreResult<Option<Value>> {
        Ok(self.peek_loaded(player_id).await?.map(|loaded| loaded.data))
    }

    async fn peek_loaded(&self, player_id: i64) -> StoreResult<Option<Loaded>> {
        let Some(record) = self.store.get_record(player_id).await? else { return Ok(None) };
        let mut data = record.data;
        normalize_player(&mut data);
        if let Some(root) = data.as_object_mut() {
            root.insert("timestamp".into(), Value::from(now_ts()));
        }
        Ok(Some(Loaded { data, config: self.game.player_config(record.kompu_offers), merged_assists: false }))
    }

    pub async fn load_player(&self, player_id: i64) -> StoreResult<Option<Loaded>> {
        let Some(mut loaded) = self.peek_loaded(player_id).await? else { return Ok(None) };
        loaded.merged_assists = self.merge_pending_assists(player_id, &mut loaded.data).await?;
        Ok(Some(loaded))
    }

    async fn merge_pending_assists(&self, player_id: i64, data: &mut Value) -> StoreResult<bool> {
        if !self.pending_targets.lock().remove(&player_id) {
            return Ok(false);
        }
        let assists = self.store.take_pending_assists(player_id).await?;
        if assists.len() >= ASSIST_BATCH {
            self.pending_targets.lock().insert(player_id);
        }
        if assists.is_empty() {
            return Ok(false);
        }
        merge_assists(data, assists);
        Ok(true)
    }

    pub async fn save_player(&self, player_id: i64, data: &Value) -> StoreResult<()> {
        self.store.save_data(player_id, data).await
    }

    pub async fn add_pending_assist(&self, target_id: i64, helper_id: i64, clicks: &[Value]) -> StoreResult<()> {
        self.store.add_pending_assist(target_id, helper_id, clicks).await?;
        self.pending_targets.lock().insert(target_id);
        Ok(())
    }

    pub async fn save_kompu_offers(&self, player_id: i64, offers: &Value) -> StoreResult<()> {
        self.store.set_kompu_offers(player_id, offers).await
    }

    pub async fn get_market(&self, player_id: i64) -> StoreResult<Option<Value>> {
        self.store.get_market(player_id).await
    }

    pub async fn save_market(&self, player_id: i64, market: &Value) -> StoreResult<()> {
        self.store.save_market(player_id, market).await
    }

    pub async fn get_last_minigame_ts(&self, player_id: i64) -> StoreResult<i64> {
        self.store.get_minigame_ts(player_id).await
    }

    pub async fn set_last_minigame_ts(&self, player_id: i64, timestamp: i64) -> StoreResult<()> {
        self.store.set_minigame_ts(player_id, timestamp).await
    }

    pub async fn players_summaries(&self, ids: &[i64]) -> StoreResult<HashMap<i64, Summary>> {
        let found = self.store.summaries(ids).await?;
        Ok(found.into_iter().map(|(id, data)| (id, player_summary(&data))).collect())
    }
}

#[derive(Clone, Debug)]
pub struct Summary {
    pub name: String,
    pub level: i64,
    pub gold: i64,
    pub cash: i64,
    pub food: i64,
    pub xp: i64,
    pub dragons: usize,
}

impl Summary {
    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name, "level": self.level, "gold": self.gold, "cash": self.cash,
            "food": self.food, "xp": self.xp, "dragons": self.dragons,
        })
    }
}

pub fn player_summary(data: &Value) -> Summary {
    let info = data.get("playerInfo").unwrap_or(&Value::Null);
    let number = |key: &str, default: i64| info.get(key).map(|v| int_or(v, default)).unwrap_or(default);
    Summary {
        name: info.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        level: number("level", 1),
        gold: number("gold", 0),
        cash: number("cash", 0),
        food: number("food", 0),
        xp: number("xp", 0),
        dragons: data.pointer("/map/dragons").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
    }
}

pub fn normalize_player(data: &mut Value) {
    if !data.is_object() {
        *data = Value::Object(Map::new());
    }
    let Some(root) = data.as_object_mut() else { return };

    let game_map = ensure_object(root, "map");
    {
        let store = ensure_object(game_map, "store");
        ensure_object(store, "items");
        ensure_array(store, "dragons");
    }
    for key in ["eggs", "dragons"] {
        ensure_array(game_map, key);
    }
    for key in ["ownedDragons", "work"] {
        ensure_object(game_map, key);
    }

    let private_state = ensure_object(root, "privateState");
    for key in ["kompuBoxesOpened", "achievement_points"] {
        let is_int = private_state.get(key).map(|v| v.is_i64() || v.is_u64()).unwrap_or(false);
        if !is_int {
            private_state.insert(key.to_string(), Value::from(0));
        }
    }
}

fn merge_assists(data: &mut Value, assists: Vec<PendingAssist>) {
    let Some(root) = data.as_object_mut() else { return };
    let mut rewards = 0i64;
    {
        let game_map = ensure_object(root, "map");
        let received = ensure_object(game_map, "receivedAssists");
        for assist in &assists {
            let key = format!("u{}", assist.helper);
            let entries = ensure_array(received, &key);
            rewards += assist.clicks.len() as i64;
            entries.extend(assist.clicks.iter().cloned());
            if entries.len() > MAX_RECEIVED_ASSISTS {
                let excess = entries.len() - MAX_RECEIVED_ASSISTS;
                entries.drain(..excess);
            }
        }
    }
    let private_state = ensure_object(root, "privateState");
    let left = private_state.get("assistRewardsLeft").map(|v| int_or(v, 0)).unwrap_or(0);
    private_state.insert("assistRewardsLeft".into(), Value::from(left + rewards));
}

pub fn default_player_name(email: &str) -> String {
    let local = email.split('@').next().unwrap_or("");
    let cleaned: String = local
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .take(MAX_PLAYER_NAME_LENGTH)
        .collect();
    if cleaned.is_empty() {
        "Dragonmaster".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_from_the_email() {
        assert_eq!(default_player_name("friend@example.com"), "friend");
        assert_eq!(default_player_name("a+b!c@example.com"), "abc");
        assert_eq!(default_player_name("+++@example.com"), "Dragonmaster");
        assert_eq!(default_player_name(&format!("{}@x.io", "n".repeat(40))).len(), 24);
    }

    #[test]
    fn normalizing_repairs_php_style_empty_maps() {
        let mut data = json!({"map": {"store": [], "eggs": {}, "ownedDragons": [], "work": []}, "privateState": {}});
        normalize_player(&mut data);
        assert_eq!(data["map"]["store"], json!({"items": {}, "dragons": []}));
        assert_eq!(data["map"]["eggs"], json!([]));
        assert_eq!(data["map"]["ownedDragons"], json!({}));
        assert_eq!(data["privateState"]["achievement_points"], json!(0));
    }

    #[test]
    fn merging_assists_counts_rewards_and_caps_the_history() {
        let mut data = json!({});
        normalize_player(&mut data);
        merge_assists(&mut data, vec![PendingAssist { helper: 5, clicks: vec![json!(1), json!(2)] }]);
        assert_eq!(data["map"]["receivedAssists"]["u5"], json!([1, 2]));
        assert_eq!(data["privateState"]["assistRewardsLeft"], json!(2));
        let many: Vec<Value> = (0..300).map(Value::from).collect();
        merge_assists(&mut data, vec![PendingAssist { helper: 5, clicks: many }]);
        assert_eq!(data["map"]["receivedAssists"]["u5"].as_array().unwrap().len(), MAX_RECEIVED_ASSISTS);
        assert_eq!(data["privateState"]["assistRewardsLeft"], json!(302));
    }
}
