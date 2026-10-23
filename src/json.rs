use std::io;

use serde::Serialize;
use serde_json::ser::{Formatter, Serializer};
use serde_json::{Map, Value};

struct AsciiFormatter;

impl Formatter for AsciiFormatter {
    fn write_string_fragment<W: ?Sized + io::Write>(&mut self, writer: &mut W, fragment: &str) -> io::Result<()> {
        let mut start = 0;
        for (index, ch) in fragment.char_indices() {
            if ch.is_ascii() && ch != '\x7f' {
                continue;
            }
            writer.write_all(fragment[start..index].as_bytes())?;
            let mut units = [0u16; 2];
            for unit in ch.encode_utf16(&mut units) {
                write!(writer, "\\u{:04x}", unit)?;
            }
            start = index + ch.len_utf8();
        }
        writer.write_all(fragment[start..].as_bytes())
    }
}

pub fn to_ascii_json<T: Serialize + ?Sized>(value: &T) -> String {
    let mut out = Vec::with_capacity(1024);
    let mut serializer = Serializer::with_formatter(&mut out, AsciiFormatter);
    if value.serialize(&mut serializer).is_err() {
        return "null".to_string();
    }
    String::from_utf8(out).unwrap_or_else(|_| "null".to_string())
}

pub fn as_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| {
            let f = n.as_f64()?;
            (f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e18).then_some(f as i64)
        }),
        _ => None,
    }
}

pub fn int_or(value: &Value, default: i64) -> i64 {
    as_int(value).unwrap_or(default)
}

pub fn get<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}

pub fn path<'a>(value: &'a Value, keys: &[&str]) -> &'a Value {
    let mut current = value;
    for key in keys {
        current = get(current, key);
    }
    current
}

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub fn id_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Null => "None".to_string(),
        other => other.to_string(),
    }
}

pub fn ensure_object<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let slot = map.entry(key.to_string()).or_insert_with(|| Value::Object(Map::new()));
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    match slot {
        Value::Object(object) => object,
        _ => unreachable!("just made an object"),
    }
}

pub fn ensure_array<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Vec<Value> {
    let slot = map.entry(key.to_string()).or_insert_with(|| Value::Array(Vec::new()));
    if !slot.is_array() {
        *slot = Value::Array(Vec::new());
    }
    match slot {
        Value::Array(array) => array,
        _ => unreachable!("just made an array"),
    }
}

pub fn add_to(map: &mut Map<String, Value>, key: &str, amount: i64) -> i64 {
    let current = map.get(key).map(|v| int_or(v, 0)).unwrap_or(0);
    let updated = current.saturating_add(amount);
    map.insert(key.to_string(), Value::from(updated));
    updated
}

pub fn obj_mut(value: &mut Value) -> Option<&mut Map<String, Value>> {
    value.as_object_mut()
}
