use serde_json::Value;
use super::ctx::Ctx;
use super::validation::CmdResult;
use crate::json::{ensure_object, id_text, int_or};

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let item_id = id_text(ctx.map_item(&iso_id)?.first().unwrap_or(&Value::Null));
    ensure_object(ctx.map(), "items").remove(&iso_id);

    let stored = ensure_object(ensure_object(ctx.map(), "store"), "items");
    let count = stored.get(&item_id).map(|c| int_or(c, 0)).unwrap_or(0);
    stored.insert(item_id, Value::from(count + 1));
    Ok(())
}
