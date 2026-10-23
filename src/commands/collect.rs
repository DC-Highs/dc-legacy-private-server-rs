use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{fail, reject, CmdResult, MAX_SAFE_INT};
use crate::enums::{group, Resource};
use crate::json::{get, id_text, int_or, truthy};
use crate::util::now_ts;
use super::shared::buildings::*;

const GOLD_TOLERANCE: f64 = 2.0;

const GOLD_MARGIN: i64 = 100;

const MAX_ACCRUAL_SECONDS: i64 = 7 * 86_400;

const CROP_TIME_TOLERANCE: i64 = 5;

fn habitat_gold_cap(ctx: &Ctx<'_>, iso_id: &str, now: i64) -> i64 {
    let mut rate_per_minute = 0.0;

    for dragon in ctx.map_ref().get("dragons").and_then(Value::as_array).into_iter().flatten() {
        if id_text(get(dragon, "habitatId")) != iso_id {
            continue;
        }
        let Some(dragon_cfg) = ctx.config.item(get(dragon, "type")) else { continue };
        let level = int_or(get(dragon, "xp"), 0).div_euclid(4) + 1;
        rate_per_minute += f64_or(get(dragon_cfg, "starting_coins"), 0.0)
            + f64_or(get(dragon_cfg, "coins_added"), 0.0) * (level - 1) as f64;
    }

    let item = get(get(ctx.map_ref(), "items"), iso_id);
    let started = int_or(item.get(3).unwrap_or(&Value::Null), 0);
    let last_activity = int_or(get(item.get(ATTRIBUTES_INDEX).unwrap_or(&Value::Null), "lat"), 0);
    let last_collect = started.max(last_activity);
    let elapsed = if last_collect != 0 { (now - last_collect).max(0).min(MAX_ACCRUAL_SECONDS) } else { MAX_ACCRUAL_SECONDS };
    (rate_per_minute * (elapsed as f64 / 60.0) * GOLD_TOLERANCE) as i64 + GOLD_MARGIN
}

fn allow_tutorial_harvest_once(ctx: &mut Ctx<'_>, iso_id: &str) -> CmdResult<()> {
    let completed = ctx.info_ref().get("completed_tutorial").map(id_text).unwrap_or_else(|| "0".to_string());
    let in_tutorial = completed == "0" || completed.is_empty();

    if !in_tutorial || truthy(get(ctx.private_ref(), "tutorialCropHarvested")) {
        return reject(format!("the crop on farm {iso_id} is not ready yet"));
    }

    ctx.private_state().insert("tutorialCropHarvested".into(), Value::Bool(true));
    Ok(())
}

fn get_item_field<'c>(ctx: &'c Ctx<'_>, iso_id: &str, index: usize) -> &'c Value {
    get(get(ctx.map_ref(), "items"), iso_id).get(index).unwrap_or(&Value::Null)
}

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let now = now_ts();

    attrs_mut(ctx.map_item(&iso_id)?, json!({}));

    let type_id = ctx.map_item(&iso_id)?.first().cloned().unwrap_or(Value::Null);
    let group_is_habitat = ctx.config.item(&type_id).map(|c| get(c, "group_type").as_str() == Some(group::HABITAT)).unwrap_or(false);

    let second = ctx.args.get(1);
    let second_is_number = second.map(|v| v.is_number()).unwrap_or(false);

    if second_is_number {
        if !group_is_habitat {
            return reject(format!("object {iso_id} is not a habitat"));
        }

        let claimed = ctx.int_arg(1, "gold collected", 0, MAX_SAFE_INT)?;
        let cap = habitat_gold_cap(ctx, &iso_id, now);
        let collected = claimed.min(cap);

        if collected < claimed {
            tracing::warn!(
                "Player {} claimed {claimed} gold from habitat {iso_id} but at most {cap} was produced",
                ctx.user_id
            );
        }

        ctx.add_resource(Resource::Gold, collected);
        let item = ctx.map_item(&iso_id)?;
        set_at(item, 3, now)?;
        attrs_mut(item, json!({})).insert("lat".into(), Value::from(now));
    } else if second.and_then(Value::as_str) == Some("fc") {
        let crop = get(get(get(ctx.map_ref(), "items"), &iso_id).get(ATTRIBUTES_INDEX).unwrap_or(&Value::Null), "cp").clone();

        if !crop.is_null() {
            let crop_index = int_or(&crop, 0) - 1;
            let values = ctx.config.global("COLLECT_VALUES").as_array().cloned().unwrap_or_default();
            let xps = ctx.config.global("COLLECT_XP").as_array().cloned().unwrap_or_default();
            let minutes = ctx.config.global("COLLECT_MINUTES").as_array().cloned().unwrap_or_default();

            if crop_index >= 0 && (crop_index as usize) < values.len() {
                let index = crop_index as usize;

                if let Some(grow_minutes) = minutes.get(index) {
                    let started = int_or(get_item_field(ctx, &iso_id, 3), 0);
                    let needed = (f64_or(grow_minutes, 0.0) * 60.0) as i64;

                    if now - started + CROP_TIME_TOLERANCE < needed {
                        allow_tutorial_harvest_once(ctx, &iso_id)?;
                    }
                }

                let Some(xp) = xps.get(index) else { return fail("COLLECT_XP has no entry for this crop") };

                ctx.add_resource(Resource::Food, int_or(&values[index], 0));
                ctx.add_resource(Resource::Xp, int_or(xp, 0));
            }
            attrs_mut(ctx.map_item(&iso_id)?, json!({})).remove("cp");
        }
        
        set_at(ctx.map_item(&iso_id)?, 3, 0)?;
    } else {
        let item = ctx.map_item(&iso_id)?;
        set_at(item, 3, 0)?;
        let attrs = attrs_mut(item, json!({}));

        if attrs.contains_key("lat") {
            attrs.insert("lat".into(), Value::from(now));
        }
    }
    Ok(())
}
