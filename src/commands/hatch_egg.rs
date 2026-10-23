use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::game::map_items::clear_habitat_arrival;
use crate::json::{ensure_array, ensure_object, get, id_text, int_or};
use crate::util::now_ts;
use super::shared::dragons::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let egg_uid = ctx.uid_arg(0, "egg uid")?;
    let habitat_iso = ctx.iso_arg(2, "habitat")?;
    let habitat_uid = ctx.arg(2, "habitat")?.clone();
    let dragon_name = ctx.name_arg(3, "dragon name")?;

    require_habitat(ctx, &habitat_iso)?;
    let egg_index = ctx.find_egg(egg_uid)?;
    let egg = ctx.egg_mut(egg_index)?.clone();
    let Some(item_cfg) = ctx.config.item(get(&egg, "type")) else {
        return reject(format!("no config found for dragon {}", id_text(get(&egg, "type"))));
    };

    let server_ts = ctx.server_ts();
    clear_habitat_arrival(ctx.data, &habitat_iso, server_ts);
    ensure_array(ctx.map(), "eggs").remove(egg_index);

    ctx.add_resource(Resource::Xp, int_or(get(item_cfg, "xp"), 0));

    let dragon_type = get(&egg, "type").clone();
    ensure_array(ctx.map(), "dragons").push(json!({
        "id": get(&egg, "id"),
        "type": dragon_type,
        "xp": 0,
        "habitatId": habitat_uid,
        "name": dragon_name,
        "ts": now_ts(),
        "work_id": [],
        "nbKill": 0,
        "attack": item_cfg.get("attacks").cloned().unwrap_or_else(|| Value::from(0)),
    }));

    let owned = ensure_object(ctx.map(), "ownedDragons");
    owned.entry(id_text(&dragon_type)).or_insert_with(|| json!([1, 0, 0]));
    Ok(())
}
