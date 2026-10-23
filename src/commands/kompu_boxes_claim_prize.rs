use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::json::{ensure_array, ensure_object, get, int_or};
use super::shared::kompu::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let opened = int_or(get(ctx.private_ref(), "kompuBoxesOpened"), 0);
    let board = get(ctx.private_ref(), "kompuBoxesDragons").as_array().cloned().unwrap_or_default();
    if !(opened > 0 && board.len() as i64 >= opened) {
        return reject(format!("kompuBoxesOpened is {opened}, invalid list to claim from"));
    }

    let prize = board[opened as usize - 1].clone();
    let store = ensure_object(ctx.map(), "store");
    ensure_array(store, "dragons").push(json!({ "iid": prize, "lvl": 0 }));

    ctx.private_state().insert("kompuBoxesOpened".into(), Value::from(0));
    ctx.result = deal_new_board(ctx)?;
    Ok(())
}
