use rand::distr::weighted::WeightedIndex;
use rand::distr::Distribution;
use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::json::{ensure_array, ensure_object, get, int_or};

const MISTERY_EGG_GROUP: &str = "MISTERY_EGG";

fn mistery_tier(item_cfg: &Value) -> String {
    get(item_cfg, "name").as_str().unwrap_or("").split(' ').next().unwrap_or("").to_uppercase()
}

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let egg_uid = ctx.uid_arg(0, "egg uid")?;
    let egg_index = ctx.find_egg(egg_uid)?;
    let egg_type = get(ctx.egg_mut(egg_index)?, "type").clone();
    let item_cfg = ctx.item_config(&egg_type)?;

    if get(item_cfg, "group_type").as_str() != Some(MISTERY_EGG_GROUP) {
        return reject(format!("egg {egg_uid} is not a mystery egg"));
    }

    let tier = mistery_tier(item_cfg);
    let table = get(ctx.config.section("mistery_egg"), &tier);
    let rewards = get(table, "rewards").as_array().cloned().unwrap_or_default();
    let chances: Vec<f64> = get(table, "chances").as_array().map(|c| c.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();

    let total: f64 = chances.iter().sum();
    if rewards.is_empty() || chances.len() != rewards.len() || total <= 0.0 || chances.iter().any(|c| *c < 0.0) {
        return reject(format!("mystery egg tier {tier} has no reward table in the game config"));
    }
    let Ok(distribution) = WeightedIndex::new(&chances) else {
        return reject(format!("mystery egg tier {tier} has no reward table in the game config"));
    };
    let reward = rewards[distribution.sample(&mut rand::rng())].clone();

    ensure_array(ctx.map(), "eggs").remove(egg_index);

    if reward.is_object() {
        ctx.add_resource(Resource::Gold, int_or(get(&reward, "g"), 0));
        ctx.add_resource(Resource::Food, int_or(get(&reward, "f"), 0));
        ctx.add_resource(Resource::Cash, int_or(get(&reward, "c"), 0));
    } else {
        let store = ensure_object(ctx.map(), "store");
        ensure_array(store, "dragons").push(json!({ "iid": reward, "lvl": 0 }));
    }
    Ok(())
}
