use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::json::{ensure_array, get, int_or};
use super::shared::economy::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let expansion_id = ctx.int_arg(0, "expansion id", 0, MAX_EXPANSION_ID)?;
    let cost_index = ctx.int_arg(1, "cost index", 0, 1000)? as usize;

    if expansions_contain(ctx, expansion_id) {
        return reject(format!("expansion {expansion_id} was already purchased"));
    }

    let costs = ctx.config.global("ISLANDS_COSTS").as_array().cloned().unwrap_or_default();
    let Some(cost_item) = costs.get(cost_index) else {
        return reject(format!("cost index {cost_index} exceeds the ISLANDS_COSTS limit"));
    };
    let cost = if cost_item.is_object() { cost_item.get("value").unwrap_or(cost_item) } else { &Value::Null };

    ctx.pay(int_or(get(cost, "g"), 0), int_or(get(cost, "c"), 0), 0)?;
    ensure_array(ctx.map(), "expansions").push(Value::from(expansion_id));
    Ok(())
}
