use serde_json::{json, Value};
use super::ctx::Ctx;
use super::validation::{fail, reject, CmdResult};
use crate::game::levels::level_from_xp;
use crate::game::pvp::get_pvp_player;
use crate::json::{ensure_array, get, int_or};
use crate::util::now_ts;

const MAX_PENDING_ATTACKS: usize = 20;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let target = ctx.int_arg_min(0, "opponent id", 1)?.to_string();

    let Some(victim) = get_pvp_player(ctx.state, &target).await? else {
        return reject(format!("opponent {target} does not exist"));
    };

    let team_ids: Vec<Value> = get(get(get(&victim, "privateState"), "teams"), "pvp")
        .as_array()
        .map(|team| team.iter().map(|d| get(d, "id").clone()).collect())
        .unwrap_or_default();
    let dragons = get(get(&victim, "map"), "dragons").as_array().cloned().unwrap_or_default();

    let mut units = Vec::new();
    for dragon_id in &team_ids {
        let Some(dragon) = dragons.iter().find(|d| get(d, "id") == dragon_id) else { continue };
        let Some(dragon_type) = dragon.get("type") else { return fail("opponent dragon has no type") };
        units.push(json!({
            "name": dragon.get("name").cloned().unwrap_or_else(|| Value::from("Dragon")),
            "type": dragon_type,
            "lvl": level_from_xp(int_or(get(dragon, "xp"), 0)),
            "id": get(dragon, "id"),
        }));
    }

    let attacks_sent = ensure_array(ctx.private_state(), "attacksSent");
    attacks_sent.push(json!({ "victim_id": target, "time": now_ts() }));
    if attacks_sent.len() > MAX_PENDING_ATTACKS {
        let excess = attacks_sent.len() - MAX_PENDING_ATTACKS;
        attacks_sent.drain(..excess);
    }

    ctx.result = Value::Array(units);
    Ok(())
}
