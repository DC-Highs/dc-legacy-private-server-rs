use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::json::{ensure_array, ensure_object, get, int_or};
use crate::util::now_ts;
use super::shared::progress::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let now = now_ts();
    let Some(prize_id) = pending_prize(ctx.private_ref()).cloned() else {
        return reject("there is no pending card prize to claim");
    };

    let prize = ctx
        .config
        .section("dragon_card_prizes")
        .as_array()
        .and_then(|prizes| prizes.iter().find(|p| get(p, "id") == &prize_id));
    ensure_object(ctx.private_state(), "deus_cards").insert("pending_prize".into(), Value::Null);

    if let Some(prize) = prize {
        let reward = get(prize, "reward");
        if reward.is_object() {
            ctx.add_resource(Resource::Gold, int_or(get(reward, "g"), 0));
            ctx.add_resource(Resource::Food, int_or(get(reward, "f"), 0));
            ctx.add_resource(Resource::Cash, int_or(get(reward, "c"), 0));
        } else if reward.is_i64() || reward.is_u64() {
            let store = ensure_object(ctx.map(), "store");
            ensure_array(store, "dragons").push(json!({ "iid": reward, "lvl": 0 }));
        } else if reward.as_str() == Some("retry") {
            ctx.private_state().insert("deusFreeRetry".into(), Value::Bool(true));
            return Ok(());
        }
    }
    ctx.private_state().insert("deusCardsLastTimestamp".into(), Value::from(now));
    Ok(())
}
