use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::json::{ensure_object, int_or};
use super::shared::economy::*;

fn take_one_stored(ctx: &mut Ctx<'_>, item_id: &str) -> bool {
    let store = ensure_object(ensure_object(ctx.map(), "store"), "items");
    match store.get(item_id).map(|count| int_or(count, 0)) {
        None => false,
        Some(count) if count > 1 => {
            store.insert(item_id.to_string(), Value::from(count - 1));
            true
        }
        Some(_) => {
            store.remove(item_id);
            true
        }
    }
}

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let item_id = ctx.int_arg_min(0, "item id", 1)?.to_string();
    let item_cfg = ctx.item_config(&Value::from(item_id.clone()))?;
    let price = sell_price(item_cfg);

    if !take_one_stored(ctx, &item_id) {
        return reject(format!("the player has no item {item_id} in the warehouse"));
    }
    ctx.add_resource(Resource::Gold, price);
    Ok(())
}
