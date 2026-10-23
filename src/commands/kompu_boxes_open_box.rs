use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::json::{get, int_or};

const FIRST_BOX_PRICE: i64 = 20;

const NEXT_BOX_PRICE: i64 = 10;

const MAX_BOXES: i64 = 6;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let opened = int_or(get(ctx.private_ref(), "kompuBoxesOpened"), 0);
    if opened >= MAX_BOXES {
        return reject("all the boxes are already open");
    }
    let board_size = get(ctx.private_ref(), "kompuBoxesDragons").as_array().map(Vec::len).unwrap_or(0) as i64;
    if board_size <= opened {
        return reject("there is no kompu board to open a box from");
    }

    ctx.pay_cash(if opened == 0 { FIRST_BOX_PRICE } else { NEXT_BOX_PRICE })?;
    ctx.private_state().insert("kompuBoxesOpened".into(), Value::from(opened + 1));
    Ok(())
}
