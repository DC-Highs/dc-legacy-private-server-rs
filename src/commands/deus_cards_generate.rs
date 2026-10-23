use rand::Rng;
use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::json::{as_int, ensure_object, get, int_or, truthy};
use crate::util::now_ts;
use super::shared::progress::*;

const PRIZE_COUNT: i64 = 9;

const DEFAULT_SUPER_TRY_COST: i64 = 10;

const DEFAULT_TRY_COST: i64 = 3;

const FREE_DRAW_COOLDOWN_SECONDS: i64 = 24 * 3600;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let is_paid = if ctx.has_arg(0) { ctx.flag_arg(0, "is paid")? } else { 0 };
    let is_super = if ctx.has_arg(1) { ctx.flag_arg(1, "is super")? } else { 0 };

    if is_super == 1 {
        let cost = as_int(ctx.config.global("DRAGON_CARDS_SUPER_TRY")).unwrap_or(DEFAULT_SUPER_TRY_COST);
        ctx.pay_cash(cost)?;
    } else if is_paid == 1 {
        let cost = as_int(ctx.config.global("DRAGON_CARDS_TRY")).unwrap_or(DEFAULT_TRY_COST);
        ctx.pay_cash(cost)?;
    } else {
        let now = now_ts();
        let has_retry = truthy(get(ctx.private_ref(), "deusFreeRetry"));
        let last_claim = int_or(get(ctx.private_ref(), "deusCardsLastTimestamp"), 0);
        if !has_retry && now - last_claim < FREE_DRAW_COOLDOWN_SECONDS {
            return reject("the free card draw is not available yet");
        }
        if pending_prize(ctx.private_ref()).is_some() {
            return reject("claim the pending card prize before drawing again");
        }
        let private_state = ctx.private_state();
        private_state.insert("deusFreeRetry".into(), Value::Bool(false));
        private_state.insert("deusCardsLastTimestamp".into(), Value::from(now));
    }

    let prize_id = rand::rng().random_range(0..PRIZE_COUNT);
    ensure_object(ctx.private_state(), "deus_cards").insert("pending_prize".into(), Value::from(prize_id));
    ctx.result = Value::from(prize_id);
    Ok(())
}
