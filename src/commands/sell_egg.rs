use super::ctx::Ctx;
use super::validation::CmdResult;
use crate::enums::Resource;
use crate::json::{ensure_array, get};
use super::shared::economy::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let index = ctx.find_egg(ctx.uid_arg(0, "egg uid")?)?;
    let type_id = get(ctx.egg_mut(index)?, "type").clone();
    if let Some(item_cfg) = ctx.config.item(&type_id) {
        ctx.add_resource(Resource::Gold, sell_price(item_cfg));
    }
    ensure_array(ctx.map(), "eggs").remove(index);
    Ok(())
}
