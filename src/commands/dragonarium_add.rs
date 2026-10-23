use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::DragonariumTab;
use crate::game::dragonarium::{
    ensure_slots, MAX_SLOTS_PER_TAB,
};
use crate::json::int_or;

const SLOT_COSTS: [i64; 5] = [2, 2, 3, 4, 5];

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let slot_index = ctx.int_arg(0, "slot", 0, SLOT_COSTS.len() as i64 - 1)? as usize;
    let tab = DragonariumTab::from_index(slot_index).unwrap_or(DragonariumTab::Elemental);

    let current = int_or(&ensure_slots(ctx.private_state())[slot_index], 0);
    if current >= MAX_SLOTS_PER_TAB {
        return reject(format!("the {} tab already has the maximum number of slots", tab.name()));
    }
    ctx.pay_cash(SLOT_COSTS[slot_index])?;
    ensure_slots(ctx.private_state())[slot_index] = Value::from(current + 1);
    Ok(())
}
