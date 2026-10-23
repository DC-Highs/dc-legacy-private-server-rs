use serde_json::Value;

use crate::gamedata::PlayerConfig;
use crate::json::{get, int_or, path};

pub const EMPTY_ATTACK_SLOT: i64 = -1;
pub const ATTACK_SLOTS: usize = 4;
pub const TRAINING_TOLERANCE_SECONDS: i64 = 5;

pub fn training_time(config: &PlayerConfig, attack_id: i64) -> i64 {
    int_or(path(config.section("attacks"), &[&attack_id.to_string(), "training_time"]), 0)
}

pub fn teach_attack(dragon: &mut Value, attack_id: i64, attack_pos: usize) {
    let Some(object) = dragon.as_object_mut() else { return };
    if !object.get("attack").map(Value::is_array).unwrap_or(false) {
        object.insert("attack".into(), Value::Array(vec![Value::from(EMPTY_ATTACK_SLOT); ATTACK_SLOTS]));
    }
    if let Some(Value::Array(attacks)) = object.get_mut("attack") {
        while attacks.len() <= attack_pos {
            attacks.push(Value::from(EMPTY_ATTACK_SLOT));
        }
        attacks[attack_pos] = Value::from(attack_id);
    }
    object.insert("work_id".into(), Value::Array(Vec::new()));
}

pub fn has_attack(config: &PlayerConfig, attack_id: i64) -> bool {
    !get(config.section("attacks"), &attack_id.to_string()).is_null()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn teaching_fills_the_slot_and_clears_the_work() {
        let mut dragon = json!({"attack": 0, "work_id": 5});
        teach_attack(&mut dragon, 12, 2);
        assert_eq!(dragon["attack"], json!([-1, -1, 12, -1]));
        assert_eq!(dragon["work_id"], json!([]));
    }
}
