use super::ctx::{set_field, Ctx};
use super::validation::CmdResult;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let index = ctx.find_dragon(ctx.uid_arg(0, "dragon uid")?)?;
    let name = ctx.name_arg(1, "dragon name")?;
    set_field(ctx.dragon_mut(index)?, "name", name)?;
    Ok(())
}
