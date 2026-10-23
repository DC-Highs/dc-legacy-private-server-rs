use rand::seq::SliceRandom;
use serde_json::{json, Value};
use crate::commands::ctx::Ctx;
use crate::commands::validation::{fail, CmdResult};
use crate::game::black_market::roll_kompu_pool;

pub(crate) fn deal_new_board(ctx: &mut Ctx<'_>) -> CmdResult<Value> {
    let Some((pool, unique_included)) = roll_kompu_pool(ctx.config) else {
        return fail("the game config has no dragons to fill a kompu board");
    };
    if unique_included {
        tracing::info!("The unique dragon ({:?}) appeared in the pool", pool.last());
    }

    let mut shuffled = pool.clone();
    shuffled.shuffle(&mut rand::rng());

    let private_state = ctx.private_state();
    private_state.insert("kompuBoxesDragons".into(), json!(pool));
    private_state.insert("kompuBoxesShuffledDragons".into(), json!(shuffled));
    Ok(json!({ "kompuBoxesDragons": pool, "kompuBoxesShuffledDragons": shuffled }))
}
