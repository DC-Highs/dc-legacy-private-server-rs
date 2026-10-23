use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Command;
use crate::json::{ensure_array, get, int_or};
use super::shared::economy::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let expansion_id = ctx.int_arg(0, "expansion id", 0, MAX_EXPANSION_ID)?;

    let prices = ctx.config.section("expansion_prices").as_array().cloned().unwrap_or_default();
    let current_index = ensure_array(ctx.map(), "expansions").len();
    if current_index >= prices.len() {
        return reject(format!("no more prices defined in expansion_prices for expansion number {}", current_index + 1));
    }
    if expansions_contain(ctx, expansion_id) {
        return reject(format!("expansion {expansion_id} was already purchased"));
    }

    let price = &prices[current_index];
    if ctx.cmd == Command::ExpandGold {
        ctx.pay_gold(int_or(get(price, "coins"), 0))?;
    } else {
        ctx.pay_cash(int_or(get(price, "cash"), 0))?;
    }
    ensure_array(ctx.map(), "expansions").push(Value::from(expansion_id));
    Ok(())
}
