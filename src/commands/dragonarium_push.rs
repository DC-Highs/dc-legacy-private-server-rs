use super::ctx::{set_field, Ctx};
use super::validation::{reject, CmdResult};
use crate::enums::DragonariumTab;
use crate::game::dragonarium::{
    attribute_codes, has_free_slot, push_dragon, tab_for_attributes,
};
use crate::json::{ensure_array, get};

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let index = ctx.find_dragon(ctx.uid_arg(0, "dragon uid")?)?;
    let dragon_type = get(ctx.dragon_mut(index)?, "type").clone();

    let tab = match ctx.config.item(&dragon_type) {
        Some(cfg) => tab_for_attributes(&attribute_codes(cfg)),
        None => DragonariumTab::Elemental,
    };
    if !has_free_slot(ctx.private_state(), tab) {
        return reject(format!("the dragonarium tab {} is full", tab.name()));
    }

    let mut dragon = ensure_array(ctx.map(), "dragons").remove(index);
    set_field(&mut dragon, "habitatId", 0)?;
    push_dragon(ctx.private_state(), tab, dragon);
    Ok(())
}
