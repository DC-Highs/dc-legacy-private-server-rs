//! In-memory implementations of the stores, used by the tests (no PostgreSQL or MongoDB needed).
use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde_json::{json, Value};

use super::{
    AccountStore, Friendship, LoginSession, NewUser, PendingAssist, PlayerRecord, PlayerStore, User,
    FIRST_PLAYER_ID,
};
use crate::error::{StoreError, StoreResult};

#[derive(Default)]
struct AccountData {
    users: BTreeMap<i64, User>,
    sessions: HashMap<String, LoginSession>,
    friendships: BTreeMap<i64, Friendship>,
    settings: HashMap<String, String>,
    next_user_id: i64,
    next_friendship_id: i64,
}

pub struct MemoryAccounts {
    inner: Mutex<AccountData>,
}

impl Default for MemoryAccounts {
    fn default() -> Self {
        MemoryAccounts::new()
    }
}

impl MemoryAccounts {
    pub fn new() -> MemoryAccounts {
        MemoryAccounts {
            inner: Mutex::new(AccountData {
                next_user_id: FIRST_PLAYER_ID,
                next_friendship_id: 1,
                ..AccountData::default()
            }),
        }
    }
}

#[async_trait]
impl AccountStore for MemoryAccounts {
    async fn ping(&self) -> StoreResult<()> {
        Ok(())
    }

    async fn insert_user(&self, new: NewUser) -> StoreResult<User> {
        let mut data = self.inner.lock();
        if data.users.values().any(|u| u.email == new.email) {
            return Err(StoreError::Conflict);
        }
        let id = match new.id {
            Some(id) => {
                if data.users.contains_key(&id) {
                    return Err(StoreError::Conflict);
                }
                id
            }
            None => {
                let id = data.next_user_id;
                data.next_user_id += 1;
                id
            }
        };
        let user = User {
            id,
            email: new.email,
            password_hash: new.password_hash,
            is_admin: new.is_admin,
            is_active: new.is_active,
            created_at: Utc::now(),
            last_login_at: None,
        };
        data.users.insert(id, user.clone());
        Ok(user)
    }

    async fn get_user(&self, id: i64) -> StoreResult<Option<User>> {
        Ok(self.inner.lock().users.get(&id).cloned())
    }

    async fn get_user_by_email(&self, email: &str) -> StoreResult<Option<User>> {
        Ok(self.inner.lock().users.values().find(|u| u.email == email).cloned())
    }

    async fn update_user(&self, user: &User) -> StoreResult<()> {
        let mut data = self.inner.lock();
        if data.users.values().any(|u| u.id != user.id && u.email == user.email) {
            return Err(StoreError::Conflict);
        }
        data.users.insert(user.id, user.clone());
        Ok(())
    }

    async fn delete_user(&self, id: i64) -> StoreResult<bool> {
        let mut data = self.inner.lock();
        let existed = data.users.remove(&id).is_some();
        data.sessions.retain(|_, s| s.user_id != id);
        data.friendships.retain(|_, f| f.low_id != id && f.high_id != id && f.requester_id != id);
        Ok(existed)
    }

    async fn list_users(&self, query: &str, offset: i64, limit: i64) -> StoreResult<(Vec<User>, i64)> {
        let data = self.inner.lock();
        let needle = query.to_lowercase();
        let numeric = (!query.is_empty() && query.chars().all(|c| c.is_ascii_digit()))
            .then(|| query.parse::<i64>().ok())
            .flatten();
        let matches: Vec<&User> = data
            .users
            .values()
            .rev()
            .filter(|u| query.is_empty() || u.email.to_lowercase().contains(&needle) || Some(u.id) == numeric)
            .collect();
        let total = matches.len() as i64;
        let items = matches.into_iter().skip(offset.max(0) as usize).take(limit.max(0) as usize).cloned().collect();
        Ok((items, total))
    }

    async fn users_by_ids(&self, ids: &[i64]) -> StoreResult<Vec<User>> {
        let data = self.inner.lock();
        Ok(ids.iter().filter_map(|id| data.users.get(id).cloned()).collect())
    }

    async fn insert_session(&self, session: &LoginSession) -> StoreResult<()> {
        self.inner.lock().sessions.insert(session.token_hash.clone(), session.clone());
        Ok(())
    }

    async fn get_session(&self, token_hash: &str) -> StoreResult<Option<LoginSession>> {
        Ok(self.inner.lock().sessions.get(token_hash).cloned())
    }

    async fn touch_session(&self, token_hash: &str, last_seen: DateTime<Utc>, expires_at: DateTime<Utc>) -> StoreResult<()> {
        if let Some(session) = self.inner.lock().sessions.get_mut(token_hash) {
            session.last_seen_at = last_seen;
            session.expires_at = expires_at;
        }
        Ok(())
    }

    async fn delete_session(&self, token_hash: &str) -> StoreResult<()> {
        self.inner.lock().sessions.remove(token_hash);
        Ok(())
    }

    async fn delete_sessions_of(&self, user_id: i64) -> StoreResult<()> {
        self.inner.lock().sessions.retain(|_, s| s.user_id != user_id);
        Ok(())
    }

    async fn purge_expired_sessions(&self, now: DateTime<Utc>) -> StoreResult<u64> {
        let mut data = self.inner.lock();
        let before = data.sessions.len();
        data.sessions.retain(|_, s| s.expires_at > now);
        Ok((before - data.sessions.len()) as u64)
    }

    async fn get_friendship(&self, id: i64) -> StoreResult<Option<Friendship>> {
        Ok(self.inner.lock().friendships.get(&id).cloned())
    }

    async fn get_friendship_pair(&self, low_id: i64, high_id: i64) -> StoreResult<Option<Friendship>> {
        let data = self.inner.lock();
        Ok(data.friendships.values().find(|f| f.low_id == low_id && f.high_id == high_id).cloned())
    }

    async fn insert_friendship(&self, low_id: i64, high_id: i64, requester_id: i64, status: &str) -> StoreResult<Friendship> {
        let mut data = self.inner.lock();
        if data.friendships.values().any(|f| f.low_id == low_id && f.high_id == high_id) {
            return Err(StoreError::Conflict);
        }
        let id = data.next_friendship_id;
        data.next_friendship_id += 1;
        let friendship = Friendship {
            id,
            low_id,
            high_id,
            requester_id,
            status: status.to_string(),
            created_at: Utc::now(),
            responded_at: None,
        };
        data.friendships.insert(id, friendship.clone());
        Ok(friendship)
    }

    async fn update_friendship(&self, friendship: &Friendship) -> StoreResult<()> {
        self.inner.lock().friendships.insert(friendship.id, friendship.clone());
        Ok(())
    }

    async fn delete_friendship(&self, id: i64) -> StoreResult<()> {
        self.inner.lock().friendships.remove(&id);
        Ok(())
    }

    async fn accepted_friend_ids(&self, user_id: i64) -> StoreResult<Vec<i64>> {
        let data = self.inner.lock();
        Ok(data
            .friendships
            .values()
            .filter(|f| f.status == super::FRIEND_ACCEPTED && (f.low_id == user_id || f.high_id == user_id))
            .map(|f| if f.low_id == user_id { f.high_id } else { f.low_id })
            .collect())
    }

    async fn count_accepted(&self, user_id: i64) -> StoreResult<i64> {
        Ok(self.accepted_friend_ids(user_id).await?.len() as i64)
    }

    async fn count_pending_sent(&self, requester_id: i64) -> StoreResult<i64> {
        let data = self.inner.lock();
        Ok(data
            .friendships
            .values()
            .filter(|f| f.status == super::FRIEND_PENDING && f.requester_id == requester_id)
            .count() as i64)
    }

    async fn pending_involving(&self, user_id: i64) -> StoreResult<Vec<Friendship>> {
        let data = self.inner.lock();
        Ok(data
            .friendships
            .values()
            .rev()
            .filter(|f| f.status == super::FRIEND_PENDING && (f.low_id == user_id || f.high_id == user_id))
            .cloned()
            .collect())
    }

    async fn count_incoming(&self, user_id: i64) -> StoreResult<i64> {
        let data = self.inner.lock();
        Ok(data
            .friendships
            .values()
            .filter(|f| {
                f.status == super::FRIEND_PENDING
                    && (f.low_id == user_id || f.high_id == user_id)
                    && f.requester_id != user_id
            })
            .count() as i64)
    }

    async fn get_setting(&self, key: &str) -> StoreResult<Option<String>> {
        Ok(self.inner.lock().settings.get(key).cloned())
    }

    async fn set_setting(&self, key: &str, value: &str) -> StoreResult<()> {
        self.inner.lock().settings.insert(key.to_string(), value.to_string());
        Ok(())
    }
}

#[derive(Default)]
struct PlayerDoc {
    data: Value,
    kompu_offers: Option<Value>,
    market: Option<Value>,
    minigame_ts: i64,
}

#[derive(Default)]
struct PlayerData {
    players: HashMap<i64, PlayerDoc>,
    pending: Vec<(i64, i64, Vec<Value>)>,
    pvp_isle: Vec<(String, i64, Value)>,
    pvp_players: HashMap<String, Value>,
}

#[derive(Default)]
pub struct MemoryPlayers {
    inner: Mutex<PlayerData>,
}

impl MemoryPlayers {
    pub fn new() -> MemoryPlayers {
        MemoryPlayers::default()
    }
}

#[async_trait]
impl PlayerStore for MemoryPlayers {
    async fn ping(&self) -> StoreResult<()> {
        Ok(())
    }

    async fn ensure_indexes(&self) -> StoreResult<()> {
        Ok(())
    }

    async fn get_record(&self, id: i64) -> StoreResult<Option<PlayerRecord>> {
        Ok(self.inner.lock().players.get(&id).map(|doc| PlayerRecord {
            data: doc.data.clone(),
            kompu_offers: doc.kompu_offers.clone(),
        }))
    }

    async fn exists(&self, id: i64) -> StoreResult<bool> {
        Ok(self.inner.lock().players.contains_key(&id))
    }

    async fn get_kompu_offers(&self, id: i64) -> StoreResult<Option<Value>> {
        Ok(self.inner.lock().players.get(&id).and_then(|doc| doc.kompu_offers.clone()))
    }

    async fn create(&self, id: i64, data: &Value, replace: bool) -> StoreResult<()> {
        let mut inner = self.inner.lock();
        if replace || !inner.players.contains_key(&id) {
            inner.players.insert(id, PlayerDoc { data: data.clone(), ..PlayerDoc::default() });
        }
        Ok(())
    }

    async fn delete(&self, id: i64) -> StoreResult<()> {
        let mut inner = self.inner.lock();
        inner.players.remove(&id);
        inner.pending.retain(|(target, _, _)| *target != id);
        Ok(())
    }

    async fn save_data(&self, id: i64, data: &Value) -> StoreResult<()> {
        if let Some(doc) = self.inner.lock().players.get_mut(&id) {
            doc.data = data.clone();
        }
        Ok(())
    }

    async fn set_kompu_offers(&self, id: i64, offers: &Value) -> StoreResult<()> {
        if let Some(doc) = self.inner.lock().players.get_mut(&id) {
            doc.kompu_offers = Some(offers.clone());
        }
        Ok(())
    }

    async fn pending_targets(&self) -> StoreResult<Vec<i64>> {
        Ok(self.inner.lock().pending.iter().map(|(target, _, _)| *target).collect())
    }

    async fn take_pending_assists(&self, target: i64) -> StoreResult<Vec<PendingAssist>> {
        let mut inner = self.inner.lock();
        let mut taken = Vec::new();
        let mut kept = Vec::new();
        for entry in inner.pending.drain(..) {
            if entry.0 == target && taken.len() < 100 {
                taken.push(PendingAssist { helper: entry.1, clicks: entry.2 });
            } else {
                kept.push(entry);
            }
        }
        inner.pending = kept;
        Ok(taken)
    }

    async fn add_pending_assist(&self, target: i64, helper: i64, clicks: &[Value]) -> StoreResult<()> {
        self.inner.lock().pending.push((target, helper, clicks.to_vec()));
        Ok(())
    }

    async fn get_market(&self, id: i64) -> StoreResult<Option<Value>> {
        Ok(self.inner.lock().players.get(&id).and_then(|d| d.market.clone()))
    }

    async fn save_market(&self, id: i64, market: &Value) -> StoreResult<()> {
        if let Some(doc) = self.inner.lock().players.get_mut(&id) {
            doc.market = Some(market.clone());
        }
        Ok(())
    }

    async fn get_minigame_ts(&self, id: i64) -> StoreResult<i64> {
        Ok(self.inner.lock().players.get(&id).map(|d| d.minigame_ts).unwrap_or(0))
    }

    async fn set_minigame_ts(&self, id: i64, timestamp: i64) -> StoreResult<()> {
        if let Some(doc) = self.inner.lock().players.get_mut(&id) {
            doc.minigame_ts = timestamp;
        }
        Ok(())
    }

    async fn summaries(&self, ids: &[i64]) -> StoreResult<HashMap<i64, Value>> {
        let inner = self.inner.lock();
        Ok(ids
            .iter()
            .filter_map(|id| {
                let doc = inner.players.get(id)?;
                Some((
                    *id,
                    json!({
                        "playerInfo": doc.data.get("playerInfo").cloned().unwrap_or(Value::Null),
                        "map": { "dragons": doc.data.pointer("/map/dragons").cloned().unwrap_or(Value::Null) },
                    }),
                ))
            })
            .collect())
    }

    async fn pvp_is_seeded(&self) -> StoreResult<bool> {
        Ok(!self.inner.lock().pvp_isle.is_empty())
    }

    async fn pvp_seed(&self, isle: Vec<Value>, players: Vec<(String, Value)>) -> StoreResult<()> {
        let mut inner = self.inner.lock();
        for (order, entry) in isle.into_iter().enumerate() {
            let id = crate::json::id_text(entry.get("id").unwrap_or(&Value::Null));
            inner.pvp_isle.push((id, order as i64, entry));
        }
        inner.pvp_players.extend(players);
        Ok(())
    }

    async fn pvp_page(&self, skip: i64, limit: i64) -> StoreResult<Vec<Value>> {
        let mut entries = self.inner.lock().pvp_isle.clone();
        entries.sort_by_key(|(_, order, _)| *order);
        Ok(entries
            .into_iter()
            .skip(skip.max(0) as usize)
            .take(limit.max(0) as usize)
            .map(|(_, _, entry)| entry)
            .collect())
    }

    async fn pvp_player(&self, id: &str) -> StoreResult<Option<Value>> {
        Ok(self.inner.lock().pvp_players.get(id).cloned())
    }

    async fn pvp_entries(&self) -> StoreResult<Vec<Value>> {
        Ok(self.inner.lock().pvp_isle.iter().map(|(_, _, entry)| entry.clone()).collect())
    }

    async fn pvp_count(&self) -> StoreResult<i64> {
        Ok(self.inner.lock().pvp_isle.len() as i64)
    }

    async fn pvp_insert(&self, id: &str, order: i64, entry: Value, data: Value) -> StoreResult<()> {
        let mut inner = self.inner.lock();
        inner.pvp_isle.push((id.to_string(), order, entry));
        inner.pvp_players.insert(id.to_string(), data);
        Ok(())
    }
}
