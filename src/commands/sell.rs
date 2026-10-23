use serde_json::Value;
use super::ctx::Ctx;
use super::validation::CmdResult;
use crate::enums::Resource;
use crate::json::ensure_object;
use super::shared::economy::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let type_id = ctx.map_item(&iso_id)?.first().cloned().unwrap_or(Value::Null);

    match ctx.config.item(&type_id) {
        Some(item_cfg) => ctx.add_resource(Resource::Gold, sell_price(item_cfg)),
        None => {
            tracing::warn!("Object {} does not exist in the game config", crate::json::id_text(&type_id));
            0
        }
    };
    ensure_object(ctx.map(), "items").remove(&iso_id);
    Ok(())
}
