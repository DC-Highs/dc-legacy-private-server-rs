use super::ctx::{set_field, Ctx};
use super::validation::CmdResult;
use super::shared::dragons::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let index = ctx.find_dragon(ctx.uid_arg(0, "dragon uid")?)?;
    let habitat = ctx.int_arg_min(1, "habitat", 1)?;
    require_habitat(ctx, &habitat.to_string())?;
    set_field(ctx.dragon_mut(index)?, "habitatId", habitat)?;
    Ok(())
}
