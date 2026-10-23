use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::game::training::{teach_attack, training_time};
use crate::json::ensure_object;
use crate::util::now_ts;
use super::shared::dragons::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let building = ctx.iso_arg(0, "training building")?;
    let Some(training) = training_of(ctx, &building) else {
        return reject(format!("building {building} is not training"));
    };
    let index = ctx.find_dragon(training.dragon_uid)?;

    let total = training_time(ctx.config, training.attack_id);
    let left = (total - (now_ts() - training.started)).max(0);
    ctx.pay_cash(((left + 3599) / 3600).max(1))?;

    teach_attack(ctx.dragon_mut(index)?, training.attack_id, training.slot);
    ensure_object(ctx.map(), "work").remove(&building);
    Ok(())
}
