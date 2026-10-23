use serde_json::{json, Value};
use super::ctx::{set_field, Ctx};
use super::validation::{reject, CmdResult};
use crate::game::levels::raw_level;
use crate::json::{ensure_object, get, id_text, int_or};

const FOOD_COSTS: [i64; 41] = [
    0, 5, 10, 20, 40, 80, 160, 320, 640, 1280, 1664, 2163, 2812, 3656, 4753, 6178, 8032, 10441, 13574, 17646,
    22940, 29822, 38768, 50398, 65518, 88173, 110725, 143943, 187126, 243264, 316243, 411115, 534450, 694785,
    903220, 1174187, 1526443, 1984375, 2579688, 3353594, 4359673,
];

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let index = ctx.find_dragon(ctx.uid_arg(0, "dragon uid")?)?;
    let dragon = ctx.dragon_mut(index)?;
    let current_xp = int_or(get(dragon, "xp"), 0);
    let current_level = raw_level(current_xp);
    let dragon_type = id_text(get(dragon, "type"));

    if current_level < 0 || current_level as usize >= FOOD_COSTS.len() {
        return reject(format!("dragon {} already reached the highest registered level", id_text(get(dragon, "id"))));
    }
    ctx.pay_food(FOOD_COSTS[current_level as usize])?;

    let new_xp = current_xp + 1;
    set_field(ctx.dragon_mut(index)?, "xp", new_xp)?;
    let new_level = raw_level(new_xp);

    let owned = ensure_object(ctx.map(), "ownedDragons");
    let entry = owned.entry(dragon_type).or_insert_with(|| json!([1, 0, 0]));
    if let Some(book) = entry.as_array_mut() {
        while book.len() < 3 {
            book.push(Value::from(0));
        }
        if new_level >= 4 {
            book[1] = Value::from(2);
        }
        if new_level >= 7 {
            book[2] = Value::from(3);
        }
    }
    Ok(())
}
