use serde_json::{json, Value};
use super::ctx::{cost_of, Ctx};
use super::validation::{reject, CmdResult};
use crate::enums::{group, Resource};
use crate::json::{ensure_array, get, int_or, truthy};
use crate::names::random_dragon_name;
use crate::util::now_ts;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let egg_uid = ctx.new_uid(0, "egg uid")?;
    let dragon_id = ctx.int_arg_min(1, "dragon id", 1)?;

    let item_cfg = ctx.item_config(&Value::from(dragon_id))?;
    let costs = get(item_cfg, "costs");

    if get(item_cfg, "group_type").as_str() != Some(group::DRAGON) {
        return reject(format!("item {dragon_id} is not a dragon"));
    }

    if !truthy(costs) {
        return reject(format!("dragon {dragon_id} is not for sale"));
    }

    let (gold, cash, food) = cost_of(costs);
    
    ctx.pay(gold, cash, food)?;
    ctx.add_resource(Resource::Xp, int_or(get(item_cfg, "xp"), 0));

    ensure_array(ctx.map(), "eggs").push(json!({
        "id": egg_uid, "type": dragon_id, "name": random_dragon_name(), "xp": 0, "hatched": false, "ts": now_ts(),
    }));
    Ok(())
}
