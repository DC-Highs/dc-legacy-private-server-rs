use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::json::{as_int, ensure_array, get, truthy};
use super::shared::progress::*;

const GOAL_ACHIEVEMENT_POINTS: i64 = 35;

const MAX_GOAL_ID: i64 = 9999;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let goal_id = ctx.int_arg(0, "goal id", 0, MAX_GOAL_ID)?;
    let goal = get(ctx.config.section("goals"), &goal_id.to_string());

    if !truthy(goal) {
        return reject(format!("goal {goal_id} does not exist in the game config"));
    }

    let completed = ensure_array(ctx.private_state(), "completedGoals");

    if completed.iter().any(|g| as_int(g) == Some(goal_id)) {
        return reject(format!("goal {goal_id} was already completed"));
    }
    
    completed.push(Value::from(goal_id));

    grant_reward(ctx, get(goal, "reward"), true);
    add_points(ctx, GOAL_ACHIEVEMENT_POINTS);
    Ok(())
}
