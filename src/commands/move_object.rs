use super::ctx::Ctx;
use super::validation::{CmdResult, MAX_COORDINATE, MAX_SMALL_INT};
use super::shared::buildings::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let tx = ctx.int_arg(1, "x", 0, MAX_COORDINATE)?;
    let ty = ctx.int_arg(2, "y", 0, MAX_COORDINATE)?;
    let frame = ctx.int_arg(3, "frame", 0, MAX_SMALL_INT)?;

    let item = ctx.map_item(&iso_id)?;
    set_at(item, 1, tx)?;
    set_at(item, 2, ty)?;
    set_at(item, 4, frame)
}
