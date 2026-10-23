use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::json::{as_int, ensure_array, get, id_text, truthy};
use super::shared::progress::*;

const COLLECTION_ACHIEVEMENT_POINTS: i64 = 75;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let collection_id = ctx.int_arg(0, "collection id", 0, 100_000)?;
    let collection = ctx
        .config
        .section("collections")
        .as_array()
        .and_then(|list| list.iter().find(|c| as_int(get(c, "id")) == Some(collection_id)));
    let Some(collection) = collection.filter(|c| truthy(c)) else {
        return reject(format!("collection {collection_id} does not exist in the game config"));
    };

    if ensure_array(ctx.private_state(), "completedCollection").iter().any(|c| as_int(c) == Some(collection_id)) {
        return reject(format!("the player had already completed collection {collection_id}"));
    }

    let owned = get(ctx.map_ref(), "ownedDragons");
    let missing: Vec<String> = get(collection, "dragon_ids")
        .as_array()
        .map(|ids| ids.iter().map(id_text).filter(|id| owned.get(id.as_str()).is_none()).collect())
        .unwrap_or_default();
    if !missing.is_empty() {
        return reject(format!("collection {collection_id} is incomplete, missing dragons {missing:?}"));
    }

    ensure_array(ctx.private_state(), "completedCollection").push(Value::from(collection_id));
    grant_reward(ctx, get(collection, "reward"), false);
    add_points(ctx, COLLECTION_ACHIEVEMENT_POINTS);
    Ok(())
}
