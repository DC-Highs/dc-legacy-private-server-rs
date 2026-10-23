use serde_json::{json, Value};
use super::ctx::{set_field, Ctx};
use super::validation::{reject, CmdResult};
use crate::game::training::{has_attack, ATTACK_SLOTS};
use crate::json::{as_int, ensure_object, get, truthy};
use crate::util::now_ts;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let building = ctx.iso_arg(0, "training building")?;
    let dragon_uid = ctx.uid_arg(1, "dragon uid")?;
    let attack_id = ctx.int_arg(2, "attack id", 1, 100_000)?;
    let attack_pos = ctx.int_arg(3, "attack slot", 0, ATTACK_SLOTS as i64 - 1)?;

    ctx.map_item(&building)?;
    if truthy(get(get(ctx.map_ref(), "work"), &building)) {
        return reject(format!("building {building} is already busy"));
    }

    let index = ctx.find_dragon(dragon_uid)?;
    if !has_attack(ctx.config, attack_id) {
        return reject(format!("attack {attack_id} does not exist"));
    }
    let dragon_type = get(ctx.dragon_mut(index)?, "type").clone();
    if let Some(trainable) = ctx.config.item(&dragon_type).map(|cfg| get(cfg, "trainable_attacks")).and_then(Value::as_array) {
        if !trainable.is_empty() && !trainable.iter().any(|a| as_int(a) == Some(attack_id)) {
            return reject(format!("dragon {dragon_uid} cannot learn attack {attack_id}"));
        }
    }

    ensure_object(ctx.map(), "work").insert(building.clone(), json!([dragon_uid, attack_id, now_ts(), attack_pos]));
    set_field(ctx.dragon_mut(index)?, "work_id", building.parse::<i64>().unwrap_or(0))?;
    Ok(())
}
