use serde_json::{json, Value};
use super::ctx::{cost_of, Ctx};
use super::validation::{reject, CmdResult, MAX_COORDINATE, MAX_SMALL_INT};
use crate::enums::{group, Resource};
use crate::json::{ensure_object, get, int_or, truthy};

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let item_id = ctx.int_arg_min(1, "item id", 1)?;
    let tx = ctx.int_arg(2, "x", 0, MAX_COORDINATE)?;
    let ty = ctx.int_arg(3, "y", 0, MAX_COORDINATE)?;
    let parent_id = ctx.int_arg(4, "parent id", 0, MAX_SMALL_INT)?;
    let frame = ctx.int_arg(5, "frame", 0, MAX_SMALL_INT)?;
    let flag = ctx.int_arg(6, "flag", 0, MAX_SMALL_INT)?;

    let item_cfg = ctx.item_config(&Value::from(item_id))?;
    let group_type = get(item_cfg, "group_type").as_str().unwrap_or("");
    let costs = get(item_cfg, "costs");

    if group_type == group::DRAGON {
        return reject(format!("item {item_id} is a dragon, eggs are bought with buy_egg"));
    }

    if !truthy(costs) {
        return reject(format!("item {item_id} is not for sale"));
    }

    if ctx.has_map_item(&iso_id) {
        return reject(format!("iso id {iso_id} is already used by another object"));
    }

    let min_level = int_or(get(item_cfg, "min_level"), 1);

    if min_level > int_or(get(ctx.info_ref(), "level"), 1) {
        return reject(format!("item {item_id} requires level {min_level}"));
    }

    let (gold, cash, food) = cost_of(costs);
    ctx.pay(gold, cash, food)?;

    let build_time = int_or(get(item_cfg, "build_time"), 0);
    let current_ts = ctx.server_ts();
    let mut attributes = serde_json::Map::new();

    if group_type == group::HABITAT {
        attributes.insert("ar".into(), Value::from(0));
        attributes.insert("lat".into(), Value::from(current_ts));
    }
    
    if build_time > 0 {
        attributes.insert("tb".into(), Value::from(current_ts + build_time));
    } else {
        ctx.add_resource(Resource::Xp, int_or(get(item_cfg, "xp"), 0));
    }

    let items = ensure_object(ctx.map(), "items");
    items.insert(iso_id, json!([item_id, tx, ty, parent_id, frame, flag, Value::Object(attributes)]));
    Ok(())
}
