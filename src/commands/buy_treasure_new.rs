use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::json::{get, int_or};

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let treasure_id = ctx.int_arg(0, "treasure id", 0, 100_000)?.to_string();

    let treasure = get(ctx.config.section("treasure_items"), &treasure_id);

    if !treasure.is_object() {
        return reject(format!("treasure {treasure_id} not found in the config (treasure_items)"));
    }

    let price = int_or(get(treasure, "price"), 0);
    let amount = int_or(get(treasure, "amount"), 0);
    let kind = get(treasure, "type").as_str().unwrap_or("");
    
    let resource = match kind {
        "g" | "gold" => Resource::Gold,
        "f" | "food" => Resource::Food,
        other => return reject(format!("treasure {treasure_id} has an unsupported resource type {other:?}")),
    };

    ctx.pay_cash(price)?;
    ctx.add_resource(resource, amount);
    Ok(())
}
