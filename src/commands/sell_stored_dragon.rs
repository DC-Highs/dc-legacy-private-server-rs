use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::json::{as_int, get};
use super::shared::economy::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let dragon_id = ctx.int_arg_min(0, "dragon id", 1)?;
    let level = ctx.int_arg(1, "dragon level", 0, 100)?;
    let is_gift = ctx.flag_arg(2, "is gift")?;

    let item_cfg = ctx.item_config(&Value::from(dragon_id))?;
    let price = sell_price(item_cfg);

    let list = if is_gift == 1 {
        ctx.private_state().get_mut("gifts").and_then(Value::as_array_mut)
    } else {
        ctx.map().get_mut("store").and_then(|s| s.get_mut("dragons")).and_then(Value::as_array_mut)
    };
    let Some(list) = list else {
        return reject(format!("target list (is_gift={is_gift}) does not exist in the player save"));
    };
    let position = list.iter().position(|d| {
        d.is_object() && as_int(get(d, "iid")) == Some(dragon_id) && get(d, "lvl").as_i64() == Some(level)
    });
    let Some(position) = position else {
        return reject(format!("dragon {dragon_id} level {level} not found in the list"));
    };
    list.remove(position);
    ctx.add_resource(Resource::Gold, price);
    Ok(())
}
