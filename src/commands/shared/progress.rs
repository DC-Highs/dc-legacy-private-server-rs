use serde_json::Value;
use crate::commands::ctx::Ctx;
use crate::enums::Resource;
use crate::json::{get, int_or};

pub(crate) fn add_points(ctx: &mut Ctx<'_>, points: i64) {
    crate::json::add_to(ctx.private_state(), "achievement_points", points);
}

pub(crate) fn grant_reward(ctx: &mut Ctx<'_>, reward: &Value, only_positive: bool) {
    for (code, resource) in [("g", Resource::Gold), ("f", Resource::Food), ("c", Resource::Cash), ("x", Resource::Xp)] {
        let amount = int_or(get(reward, code), 0);
        if !only_positive || amount > 0 {
            ctx.add_resource(resource, amount);
        }
    }
}

pub(crate) fn pending_prize(private_state: &Value) -> Option<&Value> {
    get(private_state, "deus_cards").get("pending_prize").filter(|p| !p.is_null())
}
