use serde_json::{Map, Value};

use crate::enums::{attr, DragonariumTab};
use crate::json::{ensure_array, int_or};

pub const DEFAULT_SLOTS: [i64; 5] = [2, 2, 2, 1, 1];
pub const TAB_COUNT: usize = 5;
pub const MAX_SLOTS_PER_TAB: i64 = 50;

pub fn tab_for_attributes(attributes: &[&str]) -> DragonariumTab {
    let has = |code: &str| attributes.contains(&code);
    if has(attr::PURE) {
        DragonariumTab::Pure
    } else if has(attr::LEGENDARY) {
        DragonariumTab::Legendary
    } else if has(attr::DARK) {
        DragonariumTab::Dark
    } else if has(attr::METAL) {
        DragonariumTab::Metal
    } else {
        DragonariumTab::Elemental
    }
}

pub fn attribute_codes(item_config: &Value) -> Vec<&str> {
    item_config
        .get("attributes")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

pub fn ensure_slots(private_state: &mut Map<String, Value>) -> &mut Vec<Value> {
    let valid = private_state.get("dragonariumSlot").and_then(Value::as_array).map(|a| a.len() >= TAB_COUNT).unwrap_or(false);
    if !valid {
        private_state.insert("dragonariumSlot".into(), DEFAULT_SLOTS.iter().map(|n| Value::from(*n)).collect());
    }
    ensure_array(private_state, "dragonariumSlot")
}

pub fn ensure_tabs(private_state: &mut Map<String, Value>) -> &mut Vec<Value> {
    let valid = private_state.get("dragonarium").and_then(Value::as_array).map(|a| a.len() >= TAB_COUNT).unwrap_or(false);
    if !valid {
        private_state.insert("dragonarium".into(), Value::Array((0..TAB_COUNT).map(|_| Value::Array(Vec::new())).collect()));
    }
    ensure_array(private_state, "dragonarium")
}

pub fn has_free_slot(private_state: &mut Map<String, Value>, tab: DragonariumTab) -> bool {
    let slots = int_or(&ensure_slots(private_state)[tab.index()], 0);
    let used = ensure_tabs(private_state)[tab.index()].as_array().map(Vec::len).unwrap_or(0) as i64;
    used < slots
}

pub fn push_dragon(private_state: &mut Map<String, Value>, tab: DragonariumTab, dragon: Value) {
    if let Some(Value::Array(list)) = ensure_tabs(private_state).get_mut(tab.index()) {
        list.push(dragon);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tabs_follow_rarity_first() {
        assert_eq!(tab_for_attributes(&["f"]), DragonariumTab::Elemental);
        assert_eq!(tab_for_attributes(&["m", "f"]), DragonariumTab::Metal);
        assert_eq!(tab_for_attributes(&["d", "m"]), DragonariumTab::Dark);
        assert_eq!(tab_for_attributes(&["l", "d"]), DragonariumTab::Legendary);
        assert_eq!(tab_for_attributes(&["pu", "l"]), DragonariumTab::Pure);
    }

    #[test]
    fn slots_are_limited_per_tab() {
        let mut state = json!({}).as_object().cloned().unwrap();
        assert!(has_free_slot(&mut state, DragonariumTab::Legendary));
        push_dragon(&mut state, DragonariumTab::Legendary, json!({"id": 1}));
        assert!(!has_free_slot(&mut state, DragonariumTab::Legendary));
        assert!(has_free_slot(&mut state, DragonariumTab::Elemental));
    }
}
