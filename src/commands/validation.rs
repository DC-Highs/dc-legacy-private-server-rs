//! Validation of the untrusted arguments the client sends with each command.
use serde_json::Value;

pub const MAX_SAFE_INT: i64 = 1 << 53;
pub const MAX_NAME_LENGTH: usize = 32;
pub const MAX_COORDINATE: i64 = 4095;
pub const MAX_SMALL_INT: i64 = 100_000;

#[derive(Debug)]
pub enum CommandError {
    Rejected(String),
    Failed(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Rejected(reason) | CommandError::Failed(reason) => write!(f, "{reason}"),
        }
    }
}

impl From<crate::error::ApiError> for CommandError {
    fn from(error: crate::error::ApiError) -> Self {
        CommandError::Failed(error.message)
    }
}

impl From<crate::error::StoreError> for CommandError {
    fn from(error: crate::error::StoreError) -> Self {
        CommandError::Failed(error.to_string())
    }
}

pub type CmdResult<T> = Result<T, CommandError>;

pub fn reject<T>(reason: impl Into<String>) -> CmdResult<T> {
    Err(CommandError::Rejected(reason.into()))
}

pub fn fail<T>(reason: impl Into<String>) -> CmdResult<T> {
    Err(CommandError::Failed(reason.into()))
}

fn parse_int_text(text: &str) -> Option<i64> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix('-').unwrap_or(trimmed);
    if digits.is_empty() || digits.len() > 18 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    trimmed.parse().ok()
}

pub fn to_int(value: &Value, name: &str, minimum: i64, maximum: i64) -> CmdResult<i64> {
    let number = match value {
        Value::Number(n) => match n.as_i64() {
            Some(i) => i,
            None => match n.as_f64() {
                Some(f) if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e18 => f as i64,
                Some(f) if f.is_finite() && f.fract() == 0.0 => return reject(format!("{name} is out of range ({minimum}..{maximum})")),
                _ => return reject(format!("{name} must be an integer")),
            },
        },
        Value::String(text) => match parse_int_text(text) {
            Some(i) => i,
            None => return reject(format!("{name} must be an integer")),
        },
        _ => return reject(format!("{name} must be an integer")),
    };
    if number < minimum || number > maximum {
        return reject(format!("{name} is out of range ({minimum}..{maximum})"));
    }
    Ok(number)
}

pub fn to_uid(value: &Value, name: &str) -> CmdResult<i64> {
    to_int(value, name, 1, MAX_SAFE_INT)
}

pub fn to_flag(value: &Value, name: &str) -> CmdResult<i64> {
    to_int(value, name, 0, 1)
}

pub fn to_text(value: &Value, name: &str, max_length: usize, allow_empty: bool) -> CmdResult<String> {
    let Value::String(raw) = value else { return reject(format!("{name} must be text")) };
    let cleaned: String = raw.chars().filter(|c| !matches!(*c, '\0'..='\x1f' | '\x7f')).collect();
    let text = cleaned.trim().to_string();
    if text.chars().count() > max_length {
        return reject(format!("{name} is longer than {max_length} characters"));
    }
    if text.is_empty() && !allow_empty {
        return reject(format!("{name} must not be empty"));
    }
    Ok(text)
}

pub fn to_iso_id(value: &Value, name: &str) -> CmdResult<String> {
    Ok(to_int(value, name, 1, MAX_SAFE_INT)?.to_string())
}

pub fn check_json_depth(value: &Value, name: &str, max_depth: usize, max_items: usize) -> CmdResult<()> {
    let mut stack = vec![(value, 0usize)];
    let mut seen = 0usize;
    while let Some((current, depth)) = stack.pop() {
        seen += 1;
        if depth > max_depth || seen > max_items {
            return reject(format!("{name} is too large or too deeply nested"));
        }
        match current {
            Value::Object(object) => {
                for (key, item) in object {
                    if key.starts_with('$') || key.contains('.') {
                        return reject(format!("{name} contains an invalid key"));
                    }
                    stack.push((item, depth + 1));
                }
            }
            Value::Array(items) => stack.extend(items.iter().map(|item| (item, depth + 1))),
            Value::String(text) => {
                if text.chars().count() > 256 {
                    return reject(format!("{name} contains a text that is too long"));
                }
            }
            Value::Number(n) => {
                let bad = match n.as_i64() {
                    Some(i) => i.unsigned_abs() > MAX_SAFE_INT as u64,
                    None => match n.as_f64() {
                        Some(f) => !f.is_finite() || f.abs() > MAX_SAFE_INT as f64,
                        None => true,
                    },
                };
                if bad {
                    return reject(format!("{name} contains an invalid number"));
                }
            }
            Value::Bool(_) | Value::Null => {}
        }
    }
    Ok(())
}

pub fn parse_json_text(raw: &str, name: &str) -> CmdResult<Value> {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => Ok(value),
        Err(_) => reject(format!("{name} is not valid JSON")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn integers_come_from_numbers_and_numeric_text_only() {
        assert_eq!(to_int(&json!(5), "n", 0, 10).unwrap(), 5);
        assert_eq!(to_int(&json!("7"), "n", 0, 10).unwrap(), 7);
        assert_eq!(to_int(&json!(" -3 "), "n", -5, 10).unwrap(), -3);
        assert_eq!(to_int(&json!(4.0), "n", 0, 10).unwrap(), 4);
        for bad in [json!(true), json!(4.5), json!("3; DROP TABLE"), json!({"$gt": 1}), json!(null), json!(11), json!(1e30)] {
            assert!(to_int(&bad, "n", 0, 10).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn text_is_cleaned_and_bounded() {
        assert_eq!(to_text(&json!("  Rex\u{7} "), "name", 32, false).unwrap(), "Rex");
        assert!(to_text(&json!(""), "name", 32, false).is_err());
        assert_eq!(to_text(&json!(""), "name", 32, true).unwrap(), "");
        assert!(to_text(&json!("x".repeat(33)), "name", 32, false).is_err());
        assert!(to_text(&json!(5), "name", 32, false).is_err());
    }

    #[test]
    fn nested_json_is_bounded() {
        assert!(check_json_depth(&json!([1, [2, [3]]]), "x", 3, 64).is_ok());
        assert!(check_json_depth(&json!([[[[[1]]]]]), "x", 3, 64).is_err());
        assert!(check_json_depth(&json!({"$gt": 1}), "x", 3, 64).is_err());
        assert!(check_json_depth(&json!({"a.b": 1}), "x", 3, 64).is_err());
        assert!(check_json_depth(&json!(["x".repeat(300)]), "x", 3, 64).is_err());
    }
}
