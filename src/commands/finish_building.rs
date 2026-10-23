use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::json::{get, int_or};
use super::shared::buildings::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let _speed_up = ctx.int_arg(1, "speed up flag", 0, 1000)?;

    let item = ctx.map_item(&iso_id)?;
    let under_construction = item.get(ATTRIBUTES_INDEX).and_then(Value::as_object).map(|a| a.contains_key("tb")).unwrap_or(false);
    if !under_construction {
        return reject(format!("object {iso_id} is not under construction"));
    }
    let type_id = item.first().cloned().unwrap_or(Value::Null);
    let xp = ctx.config.item(&type_id).map(|c| int_or(get(c, "xp"), 0)).unwrap_or(0);

    ctx.add_resource(Resource::Xp, xp);
    let item = ctx.map_item(&iso_id)?;
    set_at(item, 3, 0)?;
    attrs_mut(item, json!({})).remove("tb");
    Ok(())
}
