use serde_json::Value;
use super::ctx::Ctx;
use super::validation::CmdResult;
use crate::json::{as_int, get, id_text, int_or};
use crate::util::now_ts;
use super::shared::buildings::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "unique id")?;
    let current_type = ctx.map_item(&iso_id)?.first().cloned().unwrap_or(Value::Null);

    let Some(current_cfg) = ctx.config.item(&current_type) else { return Ok(()) };
    let Some(upgrades_to) = current_cfg.get("upgrades_to") else { return Ok(()) };
    let Some(new_cfg) = ctx.config.item(upgrades_to) else { return Ok(()) };
    let Some(new_type) = as_int(upgrades_to).or_else(|| id_text(upgrades_to).parse().ok()) else { return Ok(()) };

    let build_time = int_or(get(new_cfg, "build_time"), 0);
    let now = now_ts();
    let item = ctx.map_item(&iso_id)?;
    set_at(item, 0, new_type)?;
    let attrs = attrs_mut(item, Value::from(0));
    if build_time > 0 {
        attrs.insert("tb".into(), Value::from(now + build_time));
    } else {
        attrs.remove("tb");
        set_at(item, 3, 0)?;
    }
    Ok(())
}
