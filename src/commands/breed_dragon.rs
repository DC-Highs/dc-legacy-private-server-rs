use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::game::breeding::get_breed_result;
use crate::json::{as_int, ensure_object, get, int_or};

const DEFAULT_BREEDING_SECONDS: i64 = 3600;

const DEFAULT_BOOST_COST: i64 = 5;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let cave_iso = ctx.iso_arg(0, "breeding cave")?;
    let parent1_id = ctx.uid_arg(1, "first parent")?;
    let parent2_id = ctx.uid_arg(2, "second parent")?;
    let use_gems = ctx.flag_arg(3, "use gems")?;

    ctx.map_item(&cave_iso)?;
    if parent1_id == parent2_id {
        return reject("a dragon cannot breed with itself");
    }
    let parent1 = ctx.dragon_mut(ctx.find_dragon(parent1_id)?)?.clone();
    let parent2 = ctx.dragon_mut(ctx.find_dragon(parent2_id)?)?.clone();
    let now = ctx.server_ts();

    let mut boosted = false;
    if use_gems == 1 {
        let boost_cost = as_int(ctx.config.global("RARITY_BOOST_COST")).unwrap_or(DEFAULT_BOOST_COST);
        if ctx.resource(Resource::Cash) >= boost_cost {
            ctx.add_resource(Resource::Cash, -boost_cost);
            boosted = true;
        } else {
            tracing::warn!("Not enough gems for the rarity boost");
        }
    }

    let level = |dragon: &Value| int_or(get(dragon, "xp"), 0).div_euclid(4) + 1;
    let db = &ctx.config.base.dragons;
    let outcome = get_breed_result(
        as_int(get(&parent1, "type")),
        as_int(get(&parent2, "type")),
        level(&parent1),
        level(&parent2),
        db,
        boosted,
    );
    let duration = db.get(outcome).map(|d| d.breeding_time).unwrap_or(DEFAULT_BREEDING_SECONDS);

    ensure_object(ctx.map(), "work").insert(cave_iso.clone(), json!([parent1_id, parent2_id, now, duration, outcome]));
    if let Some(slot) = ctx.map_item(&cave_iso)?.get_mut(3) {
        *slot = Value::from(now);
    }
    ctx.result = Value::from(outcome);
    Ok(())
}
