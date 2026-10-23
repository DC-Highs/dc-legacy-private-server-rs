use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;

fn popup_treasure(id: i64) -> Option<(Resource, i64, i64)> {
    Some(match id {
        1 => (Resource::Gold, 75_000, 15),
        2 => (Resource::Gold, 200_000, 30),
        3 => (Resource::Gold, 500_000, 80),
        5 => (Resource::Food, 10_000, 15),
        6 => (Resource::Food, 22_500, 30),
        7 => (Resource::Food, 70_000, 80),
        _ => return None,
    })
}

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let treasure_id = ctx.int_arg(0, "treasure id", 0, 100_000)?;
    
    let Some((resource, amount, price)) = popup_treasure(treasure_id) else {
        return reject(format!("treasure id {treasure_id} is not registered on the server"));
    };

    ctx.pay_cash(price)?;
    ctx.add_resource(resource, amount);
    Ok(())
}
