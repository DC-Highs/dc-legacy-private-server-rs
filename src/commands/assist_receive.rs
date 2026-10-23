use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::{group, Resource};
use crate::json::get;

const ASSIST_REWARD_XP: i64 = 10;

const HABITAT_REWARD_GOLD: i64 = 10;

const FARM_REWARD_FOOD: i64 = 10;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let item_type = ctx.int_arg_min(0, "item type", 1)?;

    let left = get(ctx.private_ref(), "assistRewardsLeft").as_i64().unwrap_or(0);

    if left <= 0 {
        return reject("there are no neighbour assists to collect");
    }

    ctx.private_state().insert("assistRewardsLeft".into(), Value::from(left - 1));

    let group_type = ctx.config.item(&Value::from(item_type)).map(|cfg| get(cfg, "group_type").as_str().unwrap_or("").to_string());
    
    let (gold, food) = match group_type.as_deref() {
        Some(group::HABITAT) => (HABITAT_REWARD_GOLD, 0),
        Some(group::FARM) => (0, FARM_REWARD_FOOD),
        _ => (0, 0),
    };

    ctx.add_resource(Resource::Xp, ASSIST_REWARD_XP);
    ctx.add_resource(Resource::Gold, gold);
    ctx.add_resource(Resource::Food, food);
    Ok(())
}
