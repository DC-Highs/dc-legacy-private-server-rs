use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult, MAX_COORDINATE};
use crate::json::{ensure_object, int_or};

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let item_id = ctx.int_arg_min(1, "item id", 1)?;
    let tx = ctx.int_arg(2, "x", 0, MAX_COORDINATE)?;
    let ty = ctx.int_arg(3, "y", 0, MAX_COORDINATE)?;

    if ctx.has_map_item(&iso_id) {
        return reject(format!("iso id {iso_id} is already used by another object"));
    }

    let stored = ensure_object(ensure_object(ctx.map(), "store"), "items");
    let key = item_id.to_string();
    match stored.get(&key).map(|c| int_or(c, 0)) {
        None => return reject(format!("object {item_id} is not in the warehouse")),
        Some(count) if count > 1 => {
            stored.insert(key, Value::from(count - 1));
        }
        Some(_) => {
            stored.remove(&key);
        }
    }

    ensure_object(ctx.map(), "items").insert(iso_id, json!([item_id, tx, ty, 0, 0, 1, {}]));
    Ok(())
}
