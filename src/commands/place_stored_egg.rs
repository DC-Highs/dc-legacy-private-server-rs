use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::group;
use crate::json::{as_int, ensure_array, ensure_object, get, int_or};
use crate::names::random_dragon_name;
use crate::util::now_ts;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let egg_uid = ctx.new_uid(0, "egg uid")?;
    let dragon_id = ctx.int_arg_min(1, "dragon id", 1)?;
    let is_gift = ctx.flag_arg(2, "is gift")?;

    let item_cfg = ctx.item_config(&Value::from(dragon_id))?;
    if get(item_cfg, "group_type").as_str() != Some(group::DRAGON) {
        return reject(format!("item {dragon_id} is not a dragon"));
    }

    let mut removed = false;
    if is_gift == 1 {
        if ctx.private_ref().get("giftsi").is_some() {
            if let Some(gifts) = ctx.private_state().get_mut("gifts").and_then(Value::as_array_mut) {
                let position = gifts.iter().position(|g| {
                    as_int(get(g, "iid")) == Some(dragon_id) || as_int(get(g, "type")) == Some(dragon_id)
                });
                if let Some(position) = position {
                    gifts.remove(position);
                    removed = true;
                }
            }
        }
    } else {
        let store = ensure_object(ctx.map(), "store");
        let dragons = ensure_array(store, "dragons");
        if let Some(position) = dragons.iter().position(|d| as_int(get(d, "iid")) == Some(dragon_id)) {
            dragons.remove(position);
            removed = true;
        }
        if !removed {
            let items = ensure_object(store, "items");
            let key = dragon_id.to_string();
            if let Some(count) = items.get(&key).map(|c| int_or(c, 0)) {
                if count > 1 {
                    items.insert(key, Value::from(count - 1));
                } else {
                    items.remove(&key);
                }
                removed = true;
            }
        }
    }
    if !removed {
        return reject(format!("egg {dragon_id} not found in the warehouse"));
    }

    ensure_array(ctx.map(), "eggs").push(json!({
        "id": egg_uid, "type": dragon_id, "name": random_dragon_name(), "xp": 0, "hatched": false, "ts": now_ts(),
    }));
    Ok(())
}
