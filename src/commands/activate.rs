use serde_json::{json, Value};
use super::ctx::{cost_of, Ctx};
use super::validation::{reject, CmdResult};
use crate::enums::group;
use crate::json::get;
use crate::util::now_ts;
use super::shared::buildings::*;

fn has_property(properties: &Value, name: &str) -> bool {
    match properties {
        Value::Object(object) => object.contains_key(name),
        Value::Array(items) => items.iter().any(|v| v.as_str() == Some(name)),
        _ => false,
    }
}

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let crop = ctx.int_arg(1, "crop", 0, 1000)?;

    let type_id = ctx.map_item(&iso_id)?.first().cloned().unwrap_or(Value::Null);
    let item_cfg = ctx.item_config(&type_id)?;
    let group_type = get(item_cfg, "group_type").as_str().unwrap_or("");
    let properties = get(item_cfg, "properties");
    let now = now_ts();

    let mut started = None;
    let mut crop_set = None;

    if group_type == group::FARM || has_property(properties, "multiCollectable") {
        if crop > 0 {
            let prices = ctx.config.global("COLLECT_PRICE").as_array().cloned().unwrap_or_default();
            if crop as usize > prices.len() {
                return reject(format!("crop {crop} does not exist"));
            }
            let (gold, cash, _) = cost_of(&prices[crop as usize - 1]);
            ctx.pay(gold, cash, 0)?;
            crop_set = Some(crop);
            started = Some(now);
        } else {
            crop_set = Some(0);
            started = Some(0);
        }
    } else if group_type == group::OBSTACLE {
        let (gold, cash, _) = cost_of(get(item_cfg, "costs"));
        ctx.pay(gold, cash, 0)?;
        started = Some(now);
    }

    let item = ctx.map_item(&iso_id)?;
    let attrs = attrs_mut(item, json!({}));

    if let Some(crop) = crop_set {
        attrs.insert("cp".into(), Value::from(crop));
    }

    if let Some(started) = started {
        set_at(item, 3, started)?;
    }
    
    Ok(())
}
