use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{check_json_depth, parse_json_text, CmdResult};
use crate::json::ensure_array;

const GOALS_SLOTS: usize = 170;

const MAX_SET_GOAL_ID: i64 = 999;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let goal_id = ctx.int_arg(0, "goal id", 0, MAX_SET_GOAL_ID)? as usize;
    let raw_state = ctx.text_arg(1, "goal state", 256, true)?;
    let goal_state = parse_json_text(&raw_state, "goal state")?;
    check_json_depth(&goal_state, "goal state", 2, 32)?;

    let private_state = ctx.private_state();
    if !private_state.get("goals").map(Value::is_array).unwrap_or(false) {
        private_state.insert("goals".into(), Value::Array(vec![Value::Null; GOALS_SLOTS]));
    }
    let goals = ensure_array(private_state, "goals");
    while goals.len() <= goal_id {
        goals.push(Value::Null);
    }
    goals[goal_id] = goal_state;
    Ok(())
}
