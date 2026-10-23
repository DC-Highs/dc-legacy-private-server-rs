use serde_json::{Map, Value};

use super::validation::{
    fail, reject, to_flag, to_int, to_iso_id, to_text, to_uid, CmdResult, MAX_NAME_LENGTH, MAX_SAFE_INT,
};
use crate::enums::{Command, Resource};
use crate::gamedata::PlayerConfig;
use crate::json::{as_int, get, int_or};
use crate::state::AppState;
use crate::util::now_ts;

pub const RESULT_OK: &str = "ok";

pub struct Ctx<'a> {
    pub state: &'a AppState,
    pub user_id: i64,
    pub cmd: Command,
    pub args: &'a [Value],
    pub data: &'a mut Value,
    pub config: &'a PlayerConfig,
    pub result: Value,
}

fn section<'v>(data: &'v mut Value, key: &str) -> &'v mut Map<String, Value> {
    match data.get_mut(key).and_then(Value::as_object_mut) {
        Some(object) => object,
        None => unreachable!("the dispatcher guarantees the `{key}` section"),
    }
}

impl<'a> Ctx<'a> {

    pub fn info(&mut self) -> &mut Map<String, Value> {
        section(self.data, "playerInfo")
    }

    pub fn private_state(&mut self) -> &mut Map<String, Value> {
        section(self.data, "privateState")
    }

    pub fn map(&mut self) -> &mut Map<String, Value> {
        section(self.data, "map")
    }

    pub fn info_ref(&self) -> &Value {
        get(self.data, "playerInfo")
    }

    pub fn private_ref(&self) -> &Value {
        get(self.data, "privateState")
    }

    pub fn map_ref(&self) -> &Value {
        get(self.data, "map")
    }

    pub fn server_ts(&self) -> i64 {
        self.data.get("timestamp").and_then(as_int).unwrap_or_else(now_ts)
    }

    pub fn resource(&self, resource: Resource) -> i64 {
        int_or(get(self.info_ref(), resource.key()), 0)
    }

    pub fn add_resource(&mut self, resource: Resource, amount: i64) -> i64 {
        crate::json::add_to(self.info(), resource.key(), amount)
    }

    pub fn arg(&self, index: usize, name: &str) -> CmdResult<&'a Value> {
        match self.args.get(index) {
            Some(value) => Ok(value),
            None => reject(format!("missing {name} (argument {index})")),
        }
    }

    pub fn has_arg(&self, index: usize) -> bool {
        index < self.args.len()
    }

    pub fn int_arg(&self, index: usize, name: &str, minimum: i64, maximum: i64) -> CmdResult<i64> {
        to_int(self.arg(index, name)?, name, minimum, maximum)
    }

    pub fn int_arg_min(&self, index: usize, name: &str, minimum: i64) -> CmdResult<i64> {
        self.int_arg(index, name, minimum, MAX_SAFE_INT)
    }

    pub fn uid_arg(&self, index: usize, name: &str) -> CmdResult<i64> {
        to_uid(self.arg(index, name)?, name)
    }

    pub fn flag_arg(&self, index: usize, name: &str) -> CmdResult<i64> {
        to_flag(self.arg(index, name)?, name)
    }

    pub fn iso_arg(&self, index: usize, name: &str) -> CmdResult<String> {
        to_iso_id(self.arg(index, name)?, name)
    }

    pub fn text_arg(&self, index: usize, name: &str, max_length: usize, allow_empty: bool) -> CmdResult<String> {
        to_text(self.arg(index, name)?, name, max_length, allow_empty)
    }

    pub fn name_arg(&self, index: usize, name: &str) -> CmdResult<String> {
        self.text_arg(index, name, MAX_NAME_LENGTH, false)
    }

    pub fn item_config(&self, item_id: &Value) -> CmdResult<&'a Value> {
        match self.config.item(item_id) {
            Some(item) => Ok(item),
            None => reject(format!("item {} does not exist in the game config", crate::json::id_text(item_id))),
        }
    }

    pub fn map_item(&mut self, iso_id: &str) -> CmdResult<&mut Vec<Value>> {
        match self.map().get_mut("items").and_then(Value::as_object_mut).and_then(|items| items.get_mut(iso_id)) {
            Some(Value::Array(item)) => Ok(item),
            Some(_) => fail(format!("object {iso_id} is malformed")),
            None => reject(format!("object {iso_id} is not on the map")),
        }
    }

    pub fn has_map_item(&self, iso_id: &str) -> bool {
        get(get(self.map_ref(), "items"), iso_id).is_array()
    }

    pub fn item_type_at(&self, iso_id: &str) -> Option<&'a Value> {
        let type_id = get(get(self.map_ref(), "items"), iso_id).get(0)?;
        self.config.item(type_id)
    }

    fn position_of(&self, list: &str, uid: i64) -> Option<usize> {
        get(self.map_ref(), list)
            .as_array()?
            .iter()
            .position(|entry| get(entry, "id").as_str().map(|s| s == uid.to_string()).unwrap_or_else(|| as_int(get(entry, "id")) == Some(uid)))
    }

    pub fn find_dragon(&self, dragon_uid: i64) -> CmdResult<usize> {
        match self.position_of("dragons", dragon_uid) {
            Some(index) => Ok(index),
            None => reject(format!("dragon {dragon_uid} is not on the map")),
        }
    }

    pub fn find_egg(&self, egg_uid: i64) -> CmdResult<usize> {
        match self.position_of("eggs", egg_uid) {
            Some(index) => Ok(index),
            None => reject(format!("egg {egg_uid} is not in the hatchery")),
        }
    }

    pub fn dragon_mut(&mut self, index: usize) -> CmdResult<&mut Value> {
        match self.map().get_mut("dragons").and_then(Value::as_array_mut).and_then(|d| d.get_mut(index)) {
            Some(dragon) => Ok(dragon),
            None => fail("dragon index out of range"),
        }
    }

    pub fn egg_mut(&mut self, index: usize) -> CmdResult<&mut Value> {
        match self.map().get_mut("eggs").and_then(Value::as_array_mut).and_then(|e| e.get_mut(index)) {
            Some(egg) => Ok(egg),
            None => fail("egg index out of range"),
        }
    }

    pub fn new_uid(&self, index: usize, name: &str) -> CmdResult<i64> {
        let uid = self.uid_arg(index, name)?;
        let matches = |entry: &Value| as_int(get(entry, "id")) == Some(uid);
        let map = self.map_ref();
        let mut in_use = ["eggs", "dragons"]
            .iter()
            .any(|list| get(map, list).as_array().map(|a| a.iter().any(matches)).unwrap_or(false));
        if !in_use {
            in_use = get(self.private_ref(), "dragonarium")
                .as_array()
                .map(|tabs| tabs.iter().any(|tab| tab.as_array().map(|a| a.iter().any(matches)).unwrap_or(false)))
                .unwrap_or(false);
        }
        if in_use {
            return reject(format!("{name} {uid} is already in use"));
        }
        Ok(uid)
    }

    pub fn pay(&mut self, gold: i64, cash: i64, food: i64) -> CmdResult<()> {
        let costs = [(Resource::Gold, gold), (Resource::Cash, cash), (Resource::Food, food)];
        for (resource, amount) in costs {
            if amount < 0 {
                return reject("negative cost");
            }
            let has = self.resource(resource);
            if has < amount {
                return reject(format!("not enough {}: needs {amount}, has {has}", resource.key()));
            }
        }
        for (resource, amount) in costs {
            if amount != 0 {
                self.add_resource(resource, -amount);
            }
        }
        Ok(())
    }

    pub fn pay_gold(&mut self, amount: i64) -> CmdResult<()> {
        self.pay(amount, 0, 0)
    }

    pub fn pay_cash(&mut self, amount: i64) -> CmdResult<()> {
        self.pay(0, amount, 0)
    }

    pub fn pay_food(&mut self, amount: i64) -> CmdResult<()> {
        self.pay(0, 0, amount)
    }
}

pub fn cost_of(costs: &Value) -> (i64, i64, i64) {
    (
        int_or(get(costs, "g"), 0),
        int_or(get(costs, "c"), 0),
        int_or(get(costs, "f"), 0),
    )
}

pub fn set_field(value: &mut Value, key: &str, new: impl Into<Value>) -> CmdResult<()> {
    match value.as_object_mut() {
        Some(object) => {
            object.insert(key.to_string(), new.into());
            Ok(())
        }
        None => fail(format!("cannot set `{key}` on something that is not an object")),
    }
}
