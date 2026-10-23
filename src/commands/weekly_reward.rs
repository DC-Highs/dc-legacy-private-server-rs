use rand::Rng;
use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{check_json_depth, parse_json_text, reject, to_int, CmdResult};
use crate::enums::Resource;
use crate::json::{ensure_array, get, int_or};
use crate::util::now_ts;

const FALLBACK_DRAGON_ID: i64 = 1011;

const MIN_SECONDS_BETWEEN_REWARDS: i64 = 5 * 86_400;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let raw = ctx.text_arg(0, "reward choice", 64, false)?;
    let reward_args = parse_json_text(&raw, "reward choice")?;
    let Some(list) = reward_args.as_array().filter(|l| (1..=2).contains(&l.len())) else {
        return reject("reward choice must be a list of one or two numbers");
    };
    check_json_depth(&reward_args, "reward choice", 1, 3)?;
    let reward_index = to_int(&list[0], "reward index", 0, 100)? as usize;
    let dragon_index = match list.get(1) {
        Some(value) => to_int(value, "dragon index", 0, 100)? as usize,
        None => 0,
    };

    let now = now_ts();
    let last_claim = int_or(get(ctx.private_ref(), "timeStampMondayBonus"), 0);
    if now - last_claim < MIN_SECONDS_BETWEEN_REWARDS {
        return reject("the weekly reward was already claimed");
    }

    let rewards = ctx.config.global("MONDAY_BONUS_REWARDS").as_array().cloned().unwrap_or_default();
    let dragons = ctx.config.global("MONDAY_BONUS_REWARDS_DRAGONS").as_array().cloned().unwrap_or_default();
    let Some(reward) = rewards.get(reward_index) else {
        return reject(format!("reward index {reward_index} does not exist in the global config"));
    };

    let value = int_or(get(reward, "value"), 0);
    match get(reward, "type").as_str() {
        Some("g") => {
            ctx.add_resource(Resource::Gold, value);
        }
        Some("c") => {
            ctx.add_resource(Resource::Cash, value);
        }
        Some("f") => {
            ctx.add_resource(Resource::Food, value);
        }
        Some("u") => {
            let dragon_id = dragons.get(dragon_index).cloned().unwrap_or_else(|| Value::from(FALLBACK_DRAGON_ID));
            let egg_uid = now + rand::rng().random_range(1000..=9999);
            ensure_array(ctx.map(), "eggs").push(json!({
                "id": egg_uid, "type": dragon_id, "name": "Bonus", "xp": 0, "hatched": false, "ts": now,
            }));
        }
        _ => {}
    }
    ctx.private_state().insert("timeStampMondayBonus".into(), Value::from(now));
    Ok(())
}
