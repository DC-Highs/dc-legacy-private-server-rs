use super::ctx::{set_field, Ctx};
use super::validation::CmdResult;
use crate::json::{get, int_or};
use crate::util::now_ts;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let index = ctx.find_egg(ctx.uid_arg(0, "egg uid")?)?;
    let egg = ctx.egg_mut(index)?.clone();
    let dragon_cfg = ctx.item_config(get(&egg, "type"))?;

    let hatching_time = int_or(get(dragon_cfg, "hatching_time"), 0);
    let started = int_or(get(&egg, "ts"), 0);
    let now = now_ts();

    let remaining = (hatching_time - (now - started)).max(0);
    let gems = ((remaining + 3599) / 3600).max(1);

    ctx.pay_cash(gems)?;
    set_field(ctx.egg_mut(index)?, "ts", now - hatching_time)?;
    Ok(())
}
