//! Combat bookkeeping shared by tournaments and PvP.
use std::collections::HashMap;

use serde_json::Value;

use crate::commands::validation::{check_json_depth, reject, to_int, CmdResult};
use crate::json::{get, id_text, int_or};

pub const MAX_KILLS_PER_DRAGON: i64 = 1000;
pub const MAX_KILLS_ENTRIES: usize = 20;

pub fn parse_kills_log(raw: &Value) -> CmdResult<HashMap<String, i64>> {
    let empty = match raw {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        _ => false,
    };
    if empty {
        return Ok(HashMap::new());
    }
    let Value::String(text) = raw else { return reject("kills log must be a short JSON text") };
    if text.chars().count() > 2000 {
        return reject("kills log must be a short JSON text");
    }
    let parsed: Value = match serde_json::from_str(text) {
        Ok(parsed) => parsed,
        Err(_) => return reject("kills log is not valid JSON"),
    };
    match &parsed {
        Value::Object(object) if object.is_empty() => return Ok(HashMap::new()),
        Value::Array(items) if items.is_empty() => return Ok(HashMap::new()),
        _ => {}
    }
    let Value::Object(object) = &parsed else { return reject("kills log must be a small object") };
    if object.len() > MAX_KILLS_ENTRIES {
        return reject("kills log must be a small object");
    }
    check_json_depth(&parsed, "kills log", 1, MAX_KILLS_ENTRIES + 1)?;
    object
        .iter()
        .map(|(key, value)| Ok((key.clone(), to_int(value, "kills", 0, MAX_KILLS_PER_DRAGON)?)))
        .collect()
}

pub fn apply_kills(data: &mut Value, kills_log: &HashMap<String, i64>) {
    if kills_log.is_empty() {
        return;
    }
    let Some(dragons) = data.pointer_mut("/map/dragons").and_then(Value::as_array_mut) else { return };
    for dragon in dragons {
        let id = id_text(get(dragon, "id"));
        if let Some(kills) = kills_log.get(&id) {
            if let Some(object) = dragon.as_object_mut() {
                let total = object.get("nbKill").map(|v| int_or(v, 0)).unwrap_or(0) + kills;
                object.insert("nbKill".into(), Value::from(total));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kill_logs_are_parsed_and_bounded() {
        assert!(parse_kills_log(&json!("")).unwrap().is_empty());
        assert!(parse_kills_log(&json!("{}")).unwrap().is_empty());
        assert_eq!(parse_kills_log(&json!("{\"7\": 3}")).unwrap()["7"], 3);
        assert!(parse_kills_log(&json!("{\"7\": 5000}")).is_err());
        assert!(parse_kills_log(&json!("not json")).is_err());
        assert!(parse_kills_log(&json!(5)).is_err());
    }

    #[test]
    fn kills_are_added_to_the_right_dragons() {
        let mut data = json!({"map": {"dragons": [{"id": 7}, {"id": 8, "nbKill": 2}]}});
        let log = HashMap::from([("8".to_string(), 3)]);
        apply_kills(&mut data, &log);
        assert_eq!(data["map"]["dragons"][0].get("nbKill"), None);
        assert_eq!(data["map"]["dragons"][1]["nbKill"], 5);
    }
}
