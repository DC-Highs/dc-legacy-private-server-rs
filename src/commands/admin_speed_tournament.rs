use serde_json::Value;
use super::ctx::Ctx;
use super::validation::CmdResult;
use crate::json::{as_int, get, int_or};

const DEFAULT_SPEED_UP_PRICE: i64 = 2;

const DEFAULT_COOLDOWN_HOURS: i64 = 12;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let price = as_int(ctx.config.global("TOURNAMENT_SPEED_UP_PRICE")).unwrap_or(DEFAULT_SPEED_UP_PRICE);
    let cooldown_hours = as_int(ctx.config.global("TOURNAMENT_COOLDOWN")).unwrap_or(DEFAULT_COOLDOWN_HOURS);

    ctx.pay_cash(price)?;
    let current = int_or(get(ctx.private_ref(), "beginTournamentTs"), 0);
    ctx.private_state().insert("beginTournamentTs".into(), Value::from((current - cooldown_hours * 3600).max(0)));
    Ok(())
}
