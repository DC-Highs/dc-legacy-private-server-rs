use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{check_json_depth, parse_json_text, reject, to_uid, CmdResult};
use crate::json::{as_int, ensure_object, get};

const MAX_TEAM_SIZE: usize = 6;

const MAX_TEAM_TYPES: usize = 8;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let team_type = ctx.text_arg(0, "team type", 16, false)?;
    let raw_ids = ctx.text_arg(1, "team ids", 256, true)?;

    let team_ids = parse_json_text(&raw_ids, "team ids")?;
    let Some(ids) = team_ids.as_array().filter(|ids| ids.len() <= MAX_TEAM_SIZE) else {
        return reject(format!("a team has at most {MAX_TEAM_SIZE} dragons"));
    };
    check_json_depth(&team_ids, "team ids", 1, MAX_TEAM_SIZE + 1)?;

    let owned: Vec<i64> = get(ctx.map_ref(), "dragons")
        .as_array()
        .map(|dragons| dragons.iter().filter_map(|d| as_int(get(d, "id"))).collect())
        .unwrap_or_default();
    let mut team = Vec::with_capacity(ids.len());
    for raw in ids {
        let dragon_id = to_uid(raw, "dragon id")?;
        if !owned.contains(&dragon_id) {
            return reject(format!("dragon {dragon_id} is not on the map"));
        }
        team.push(json!({ "id": dragon_id }));
    }

    let teams = ensure_object(ctx.private_state(), "teams");
    if !teams.contains_key(&team_type) && teams.len() >= MAX_TEAM_TYPES {
        return reject("too many team types");
    }
    teams.insert(team_type, Value::Array(team));
    Ok(())
}
