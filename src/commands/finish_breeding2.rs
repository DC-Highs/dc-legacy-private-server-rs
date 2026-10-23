use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{fail, reject, CmdResult};
use crate::json::{ensure_array, ensure_object, get, truthy};
use crate::names::random_dragon_name;
use crate::util::now_ts;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let cave_iso = ctx.iso_arg(0, "breeding cave")?;
    let egg_uid = ctx.new_uid(1, "egg uid")?;
    let _hatchery = ctx.iso_arg(2, "hatchery")?;

    let work = get(get(ctx.map_ref(), "work"), &cave_iso).clone();
    if !truthy(&work) {
        return reject(format!("no breeding data found for building {cave_iso}"));
    }
    let Some(dragon_id) = work.get(4).cloned() else { return fail("breeding data is incomplete") };

    ensure_array(ctx.map(), "eggs").push(json!({
        "id": egg_uid, "type": dragon_id, "name": random_dragon_name(), "xp": 0, "ts": now_ts(),
    }));
    ensure_object(ctx.map(), "work").remove(&cave_iso);
    if ctx.has_map_item(&cave_iso) {
        if let Some(slot) = ctx.map_item(&cave_iso)?.get_mut(3) {
            *slot = Value::from(0);
        }
    }
    ctx.result = dragon_id;
    Ok(())
}
