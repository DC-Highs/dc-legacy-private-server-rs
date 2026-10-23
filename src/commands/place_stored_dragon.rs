use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::group;
use crate::game::inventory::{register_owned_dragon, take_stored_dragon};
use crate::game::map_items::clear_habitat_arrival;
use crate::json::{ensure_array, get};
use super::shared::dragons::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let dragon_uid = ctx.new_uid(0, "dragon uid")?;
    let habitat_iso = ctx.iso_arg(1, "habitat")?;
    let name = ctx.name_arg(2, "dragon name")?;
    let dragon_type = ctx.int_arg_min(3, "dragon type", 1)?;
    let claimed_level = ctx.int_arg(4, "dragon level", 0, 100)?;
    let is_gift = ctx.flag_arg(5, "is gift")?;

    let item_cfg = ctx.item_config(&Value::from(dragon_type))?;
    if get(item_cfg, "group_type").as_str() != Some(group::DRAGON) {
        return reject(format!("item {dragon_type} is not a dragon"));
    }
    require_habitat(ctx, &habitat_iso)?;

    let (level, list_name) = take_stored_dragon(ctx.data, dragon_type, claimed_level, is_gift == 1);
    let Some(level) = level else {
        return reject(format!("dragon type {dragon_type} (level {claimed_level}) is not in {list_name}"));
    };
    let xp = 4 * level - 4;

    let server_ts = ctx.server_ts();
    clear_habitat_arrival(ctx.data, &habitat_iso, server_ts);

    ensure_array(ctx.map(), "dragons").push(json!({
        "id": dragon_uid,
        "type": dragon_type,
        "xp": xp,
        "habitatId": habitat_iso,
        "name": name,
        "ts": server_ts,
        "work_id": [],
        "nbKill": 0,
        "attack": item_cfg.get("attacks").cloned().unwrap_or_else(|| json!([])),
    }));
    register_owned_dragon(ctx.data, dragon_type, xp);
    Ok(())
}
