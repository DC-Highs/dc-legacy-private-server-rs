use serde_json::{Map, Value};
use crate::commands::validation::{fail, CmdResult};

pub(crate) const ATTRIBUTES_INDEX: usize = 6;

pub(crate) fn attrs_mut(item: &mut Vec<Value>, padding: Value) -> &mut Map<String, Value> {
    while item.len() <= ATTRIBUTES_INDEX {
        item.push(padding.clone());
    }
    if !item[ATTRIBUTES_INDEX].is_object() {
        item[ATTRIBUTES_INDEX] = Value::Object(Map::new());
    }
    match &mut item[ATTRIBUTES_INDEX] {
        Value::Object(attrs) => attrs,
        _ => unreachable!("just made an object"),
    }
}

pub(crate) fn f64_or(value: &Value, default: f64) -> f64 {
    value.as_f64().unwrap_or(default)
}

pub(crate) fn set_at(item: &mut [Value], index: usize, value: impl Into<Value>) -> CmdResult<()> {
    match item.get_mut(index) {
        Some(slot) => {
            *slot = value.into();
            Ok(())
        }
        None => fail(format!("object has no field {index}")),
    }
}
