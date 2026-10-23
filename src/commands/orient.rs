use super::ctx::Ctx;
use super::validation::{CmdResult, MAX_SMALL_INT};
use super::shared::buildings::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let frame = ctx.int_arg(1, "frame", 0, MAX_SMALL_INT)?;
    set_at(ctx.map_item(&iso_id)?, 4, frame)
}
