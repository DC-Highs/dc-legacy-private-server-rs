//! Helpers for the player's storage, gifts and owned-dragon book.
use serde_json::{json, Value};

use crate::json::{as_int, get, id_text, int_or};

const OWNED_DRAGON_STAGES: [(i64, [i64; 3]); 2] = [(24, [1, 2, 3]), (12, [1, 2, 0])];

pub const GIFTS_LIST: &str = "gifts -> dragons";
pub const STORE_LIST: &str = "store -> dragons";

fn gifts_node(data: &mut Value) -> Option<&mut Value> {
    let place = [None, Some("privateState"), Some("map")].into_iter().find(|container| match container {
        None => data.get("gifts").is_some(),
        Some(key) => get(data, key).get("gifts").is_some(),
    })?;
    match place {
        None => data.get_mut("gifts"),
        Some(key) => data.get_mut(key).and_then(|container| container.get_mut("gifts")),
    }
}

fn entry_matches(entry: &Value, dragon_type_id: i64, level: i64, is_gift: bool) -> bool {
    match entry {
        Value::Object(_) => {
            if is_gift {
                return as_int(get(entry, "iid")) == Some(dragon_type_id)
                    || as_int(get(entry, "type")) == Some(dragon_type_id);
            }
            if as_int(get(entry, "iid")) != Some(dragon_type_id) {
                return false;
            }
            let stored_level = int_or(get(entry, "lvl"), 0);
            stored_level == level || (stored_level == 0 && (level == 0 || level == 1))
        }
        Value::String(_) | Value::Number(_) => id_text(entry) == dragon_type_id.to_string(),
        _ => false,
    }
}

pub fn take_stored_dragon(
    data: &mut Value,
    dragon_type_id: i64,
    level: i64,
    is_gift: bool,
) -> (Option<i64>, &'static str) {
    let (list, list_name): (Option<&mut Value>, &'static str) = if is_gift {
        (gifts_node(data).and_then(|node| node.get_mut("dragons")), GIFTS_LIST)
    } else {
        (data.pointer_mut("/map/store/dragons"), STORE_LIST)
    };

    let Some(Value::Array(entries)) = list else { return (None, list_name) };
    let Some(index) = entries.iter().position(|entry| entry_matches(entry, dragon_type_id, level, is_gift)) else {
        return (None, list_name);
    };
    let entry = entries.remove(index);
    let stored_level = if entry.is_object() { int_or(get(&entry, "lvl"), 0) } else { 0 };
    (Some(stored_level.max(1)), list_name)
}

pub fn register_owned_dragon(data: &mut Value, dragon_type_id: i64, dragon_xp: i64) {
    let Some(owned) = data.pointer_mut("/map/ownedDragons").and_then(Value::as_object_mut) else { return };
    let key = dragon_type_id.to_string();
    if owned.contains_key(&key) {
        return;
    }
    let entry = OWNED_DRAGON_STAGES
        .iter()
        .find(|(min_xp, _)| dragon_xp >= *min_xp)
        .map(|(_, entry)| *entry)
        .unwrap_or([1, 0, 0]);
    owned.insert(key, json!(entry));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_dragons_keep_their_stored_level() {
        let mut data = json!({"map": {"store": {"dragons": [{"iid": 1000, "lvl": 3}]}}});
        assert_eq!(take_stored_dragon(&mut data, 1000, 40, false).0, None);
        assert_eq!(take_stored_dragon(&mut data, 1000, 3, false).0, Some(3));
        assert_eq!(take_stored_dragon(&mut data, 1000, 3, false).0, None);
    }

    #[test]
    fn fresh_prizes_match_level_zero_or_one() {
        let mut data = json!({"map": {"store": {"dragons": [{"iid": 5, "lvl": 0}, {"iid": 5, "lvl": 0}]}}});
        assert_eq!(take_stored_dragon(&mut data, 5, 1, false).0, Some(1));
        assert_eq!(take_stored_dragon(&mut data, 5, 0, false).0, Some(1));
    }

    #[test]
    fn gifts_are_found_wherever_the_save_keeps_them() {
        let mut data = json!({"privateState": {"gifts": {"dragons": [{"type": 7}]}}});
        assert_eq!(take_stored_dragon(&mut data, 7, 0, true).0, Some(1));
        assert_eq!(take_stored_dragon(&mut data, 7, 0, true).0, None);
    }

    #[test]
    fn the_dragon_book_grows_with_experience() {
        let mut data = json!({"map": {"ownedDragons": {}}});
        register_owned_dragon(&mut data, 1, 30);
        register_owned_dragon(&mut data, 2, 12);
        register_owned_dragon(&mut data, 3, 0);
        assert_eq!(data["map"]["ownedDragons"], json!({"1": [1, 2, 3], "2": [1, 2, 0], "3": [1, 0, 0]}));
    }
}
