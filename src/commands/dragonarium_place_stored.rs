use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::group;
use crate::game::dragonarium::{
    attribute_codes, has_free_slot, push_dragon, tab_for_attributes,
};
use crate::game::inventory::{register_owned_dragon, take_stored_dragon};
use crate::json::get;
use crate::util::now_millis;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let name = ctx.name_arg(0, "dragon name")?;
    let dragon_type = ctx.int_arg_min(1, "dragon type", 1)?;
    let claimed_level = ctx.int_arg(2, "dragon level", 0, 100)?;
    let is_gift = ctx.flag_arg(3, "is gift")?;

    let item_cfg = ctx.item_config(&Value::from(dragon_type))?;
    if get(item_cfg, "group_type").as_str() != Some(group::DRAGON) {
        return reject(format!("item {dragon_type} is not a dragon"));
    }

    let tab = tab_for_attributes(&attribute_codes(item_cfg));
    if !has_free_slot(ctx.private_state(), tab) {
        return reject(format!("the dragonarium tab {} is full", tab.name()));
    }

    let (level, list_name) = take_stored_dragon(ctx.data, dragon_type, claimed_level, is_gift == 1);
    let Some(level) = level else {
        return reject(format!("dragon type {dragon_type} (level {claimed_level}) is not in {list_name}"));
    };
    let xp = 4 * level - 4;

    let server_ts = ctx.server_ts();
    push_dragon(
        ctx.private_state(),
        tab,
        json!({
            "id": now_millis(),
            "type": dragon_type,
            "xp": xp,
            "habitatId": 0,
            "name": name,
            "ts": server_ts,
            "work_id": [],
            "nbKill": 0,
            "attack": item_cfg.get("attacks").cloned().unwrap_or_else(|| json!([])),
        }),
    );
    register_owned_dragon(ctx.data, dragon_type, xp);
    Ok(())
}
