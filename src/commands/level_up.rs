use rand::Rng;
use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::game::black_market::schedule_black_market;
use crate::json::{get, int_or};

const BLACK_MARKET_UNLOCK_LEVEL: i64 = 5;

const BLACK_MARKET_FIRST_DELAY: (i64, i64) = (86_400, 259_200);

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let new_level = ctx.int_arg(0, "level", 2, 1000)?;

    let current_level = int_or(get(ctx.info_ref(), "level"), 1);
    if new_level != current_level + 1 {
        return reject(format!("cannot go from level {current_level} to level {new_level}"));
    }

    let level_info = ctx.config.section("levels").as_array().and_then(|levels| levels.get(new_level as usize - 1));

    if let Some(info) = level_info {
        let required = int_or(get(info, "xp_required"), 0);
        let has = ctx.resource(Resource::Xp);
        if has < required {
            return reject(format!("level {new_level} needs {required} xp, the player has {has}"));
        }
    }

    ctx.info().insert("level".into(), Value::from(new_level));

    if let Some(reward) = level_info.map(|info| get(info, "reward")).and_then(Value::as_object) {
        for (code, amount) in reward {
            let resource = match code.as_str() {
                "c" => Resource::Cash,
                "g" => Resource::Gold,
                "f" => Resource::Food,
                "x" => Resource::Xp,
                _ => continue,
            };
            ctx.add_resource(resource, int_or(amount, 0));
        }
    }

    let market_ts = int_or(get(ctx.private_ref(), "BlackMarketTs"), 0);
    if new_level >= BLACK_MARKET_UNLOCK_LEVEL && market_ts == 0 {
        let delay = rand::rng().random_range(BLACK_MARKET_FIRST_DELAY.0..=BLACK_MARKET_FIRST_DELAY.1);
        let server_ts = ctx.server_ts();
        let config = ctx.config;
        schedule_black_market(ctx.private_state(), config, server_ts, delay);
    }
    Ok(())
}
