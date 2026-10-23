use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::game::training::{teach_attack, training_time, TRAINING_TOLERANCE_SECONDS};
use crate::json::ensure_object;
use crate::util::now_ts;
use super::shared::dragons::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let building = ctx.iso_arg(0, "training building")?;
    let Some(training) = training_of(ctx, &building) else {
        return reject(format!(
            "building {building} was not registered in 'work' (it probably finished already or the client is out of sync)"
        ));
    };
    let index = ctx.find_dragon(training.dragon_uid)?;

    let elapsed = now_ts() - training.started;
    if elapsed + TRAINING_TOLERANCE_SECONDS < training_time(ctx.config, training.attack_id) {
        return reject(format!("training in building {building} is not finished yet"));
    }

    teach_attack(ctx.dragon_mut(index)?, training.attack_id, training.slot);
    ensure_object(ctx.map(), "work").remove(&building);
    Ok(())
}
