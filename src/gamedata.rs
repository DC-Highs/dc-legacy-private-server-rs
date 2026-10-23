use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::Value;

use crate::game::breeding::DragonDb;
use crate::json::{get, id_text, path};
use crate::security::sign_text;

pub const GAME_CONFIG_FILE: &str = "get_game_config.json";
pub const PLAYER_TEMPLATE_FILE: &str = "get_player_info.json";

pub struct GameConfig {
    pub value: Value,
    index: HashMap<String, usize>,
    pub dragons: DragonDb,
}

impl GameConfig {
    pub fn new(value: Value) -> GameConfig {
        let mut index = HashMap::new();
        if let Some(items) = value.get("items").and_then(Value::as_array) {
            for (position, item) in items.iter().enumerate() {
                index.entry(id_text(get(item, "id"))).or_insert(position);
            }
        }
        let dragons = DragonDb::from_items(value.get("items").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]));
        GameConfig { value, index, dragons }
    }

    pub fn items(&self) -> &[Value] {
        self.value.get("items").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn item(&self, id: &Value) -> Option<&Value> {
        self.item_by_text(&id_text(id))
    }

    pub fn item_by_text(&self, id: &str) -> Option<&Value> {
        self.index.get(id).and_then(|&position| self.items().get(position))
    }

    pub fn section(&self, key: &str) -> &Value {
        get(&self.value, key)
    }

    pub fn global(&self, name: &str) -> &Value {
        path(&self.value, &["globals", name, "value"])
    }
}

pub struct PlayerConfig {
    pub base: Arc<GameConfig>,
    pub kompu_offers: Option<Value>,
}

impl PlayerConfig {
    pub fn item(&self, id: &Value) -> Option<&Value> {
        self.base.item(id)
    }

    pub fn items(&self) -> &[Value] {
        self.base.items()
    }

    pub fn section(&self, key: &str) -> &Value {
        self.base.section(key)
    }

    pub fn global(&self, name: &str) -> &Value {
        self.base.global(name)
    }

    pub fn kompu_offers(&self) -> &Value {
        self.kompu_offers.as_ref().unwrap_or_else(|| self.base.section("kompu_offers"))
    }

    pub fn signed(&self) -> String {
        sign_text(&crate::json::to_ascii_json(&WithOffers { config: self }))
    }
}

struct WithOffers<'a> {
    config: &'a PlayerConfig,
}

impl Serialize for WithOffers<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let base = &self.config.base.value;
        let Some(object) = base.as_object() else { return base.serialize(serializer) };
        let own = self.config.kompu_offers.as_ref();
        let mut map = serializer.serialize_map(None)?;
        for (key, value) in object {
            if key == "kompu_offers" && own.is_some() {
                continue;
            }
            map.serialize_entry(key, value)?;
        }
        if let Some(offers) = own {
            map.serialize_entry("kompu_offers", offers)?;
        }
        map.end()
    }
}

pub fn read_json_file(directory: &Path, name: &str) -> Result<Value, String> {
    let file = directory.join(name);
    let text = std::fs::read_to_string(&file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    parse_captured_json(&text).map_err(|e| format!("{}: {e}", file.display()))
}

pub fn parse_captured_json(text: &str) -> Result<Value, String> {
    let trimmed = text.trim_start_matches('\u{feff}');
    let body = if !trimmed.trim_start().starts_with(['{', '[']) && trimmed.contains(';') {
        trimmed.split_once(';').map(|(_, rest)| rest).unwrap_or(trimmed)
    } else {
        trimmed
    };
    serde_json::from_str(body).map_err(|e| format!("invalid JSON: {e}"))
}

pub struct GameData {
    pub config: Arc<GameConfig>,
    pub player_template: Value,
    signed_default: Bytes,
}

impl GameData {
    pub fn load(seed_dir: &Path) -> Result<GameData, String> {
        let config = read_json_file(seed_dir, GAME_CONFIG_FILE)?;
        let player_template = read_json_file(seed_dir, PLAYER_TEMPLATE_FILE)?;
        Ok(GameData::from_values(config, player_template))
    }

    pub fn from_values(config: Value, player_template: Value) -> GameData {
        let config = Arc::new(GameConfig::new(config));
        let signed_default = Bytes::from(PlayerConfig { base: config.clone(), kompu_offers: None }.signed());
        GameData { config, player_template, signed_default }
    }

    pub fn player_config(&self, kompu_offers: Option<Value>) -> PlayerConfig {
        PlayerConfig { base: self.config.clone(), kompu_offers }
    }

    pub fn signed_config(&self, config: &PlayerConfig) -> Bytes {
        if config.kompu_offers.is_none() {
            self.signed_default.clone()
        } else {
            Bytes::from(config.signed())
        }
    }
}
