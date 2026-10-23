//! Helpers for the items placed on the player's map.
use serde_json::Value;

const ATTRIBUTES_INDEX: usize = 6;

pub fn clear_habitat_arrival(data: &mut Value, habitat_id: &str, current_ts: i64) {
    let Some(item) = data
        .pointer_mut("/map/items")
        .and_then(Value::as_object_mut)
        .and_then(|items| items.get_mut(habitat_id))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    if let Some(attributes) = item.get_mut(ATTRIBUTES_INDEX).and_then(Value::as_object_mut) {
        if attributes.remove("ar").is_some() {
            attributes.insert("lat".into(), Value::from(current_ts));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_arrival_flag_becomes_a_timestamp() {
        let mut data = json!({"map": {"items": {"5": [24, 1, 1, 0, 0, 1, {"ar": 0, "tb": 9}]}}});
        clear_habitat_arrival(&mut data, "5", 123);
        assert_eq!(data["map"]["items"]["5"][6], json!({"tb": 9, "lat": 123}));
        clear_habitat_arrival(&mut data, "404", 1);
    }
}
