use super::ctx::Ctx;
use super::validation::CmdResult;
use crate::enums::Resource;
use crate::json::{ensure_array, get};
use super::shared::economy::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let index = ctx.find_dragon(ctx.uid_arg(0, "dragon uid")?)?;
    let type_id = get(ctx.dragon_mut(index)?, "type").clone();
    if let Some(item_cfg) = ctx.config.item(&type_id) {
        ctx.add_resource(Resource::Gold, sell_price(item_cfg));
    }
    ensure_array(ctx.map(), "dragons").remove(index);
    Ok(())
}
