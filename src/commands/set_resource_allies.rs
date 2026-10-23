use serde_json::{Map, Value};
use super::ctx::Ctx;
use super::validation::{check_json_depth, parse_json_text, reject, to_int, CmdResult};
use crate::enums::group;
use crate::friends;
use crate::json::{get, int_or};

const ALLIES_MARKET_ITEM_ID: i64 = 79;

const MAX_ALLY_WORKERS: usize = 6;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let iso_id = ctx.iso_arg(0, "iso id")?;
    let raw_ids = ctx.text_arg(1, "ally ids", 256, true)?;

    let type_id = ctx.map_item(&iso_id)?.first().cloned().unwrap_or(Value::Null);
    if int_or(&type_id, 0) != ALLIES_MARKET_ITEM_ID {
        return reject(format!("object {iso_id} is not the Allies Market"));
    }
    let is_social = ctx.config.item(&type_id).map(|cfg| get(cfg, "group_type").as_str() == Some(group::SOCIAL_BUILDING)).unwrap_or(false);
    if !is_social {
        return reject(format!("object {iso_id} is not a social building"));
    }

    let ally_ids = parse_json_text(&raw_ids, "ally ids")?;
    let Some(list) = ally_ids.as_array().filter(|l| l.len() <= MAX_ALLY_WORKERS) else {
        return reject(format!("the Allies Market has at most {MAX_ALLY_WORKERS} worker slots"));
    };
    check_json_depth(&ally_ids, "ally ids", 1, MAX_ALLY_WORKERS + 1)?;

    let mut friend_ids: Vec<i64> = Vec::with_capacity(list.len());
    for raw in list {
        let friend_id = to_int(raw, "ally id", 1, crate::commands::validation::MAX_SAFE_INT)?;
        if friend_ids.contains(&friend_id) {
            return reject(format!("friend {friend_id} is assigned twice"));
        }
        friend_ids.push(friend_id);
    }
    for friend_id in &friend_ids {
        if !friends::are_friends(ctx.state, ctx.user_id, *friend_id).await? {
            return reject(format!("{friend_id} is not one of the player's neighbours"));
        }
    }

    let item = ctx.map_item(&iso_id)?;
    while item.len() <= 6 {
        item.push(Value::Object(Map::new()));
    }
    if !item[6].is_object() {
        item[6] = Value::Object(Map::new());
    }
    if let Some(attrs) = item[6].as_object_mut() {
        attrs.insert("allies".into(), Value::from(friend_ids));
    }
    Ok(())
}
