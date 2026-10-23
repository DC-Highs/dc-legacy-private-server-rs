use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::game::combat::{apply_kills, parse_kills_log};
use crate::json::{ensure_array, get, id_text, int_or};
use crate::util::now_ts;

const PVP_WIN_POINTS: i64 = 35;

const LEAGUE_COMPLETE_POINTS: i64 = 120;

const LEAGUE_WIN_THRESHOLD: usize = 7;

const MIN_FIGHT_SECONDS: i64 = 10;

const MAX_FIGHT_SECONDS: i64 = 3600;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let target = ctx.int_arg_min(0, "opponent id", 1)?.to_string();
    let position = ctx.int_arg(1, "position", 0, 1000)?;
    let won = ctx.flag_arg(2, "win status")? == 1;
    let kills_log = match ctx.args.get(3) {
        Some(raw) => parse_kills_log(raw)?,
        None => Default::default(),
    };

    let now = now_ts();
    let attacks_sent = ensure_array(ctx.private_state(), "attacksSent");
    let pending = attacks_sent
        .iter()
        .rposition(|a| a.is_object() && get(a, "victim_id").as_str() == Some(target.as_str()));
    let Some(pending) = pending else {
        return reject(format!("no attack against {target} is in progress"));
    };
    let fight_seconds = now - int_or(get(&attacks_sent[pending], "time"), 0);
    if !(MIN_FIGHT_SECONDS..=MAX_FIGHT_SECONDS).contains(&fight_seconds) {
        return reject(format!("the attack against {target} lasted {fight_seconds}s, which is not plausible"));
    }
    attacks_sent.remove(pending);

    if won {
        let wins = ensure_array(ctx.map(), "universAttackWin");
        let existing = wins.iter().position(|entry| {
            entry.as_array().map(|e| e.len() >= 3 && id_text(&e[0]) == target).unwrap_or(false)
        });
        match existing {
            Some(index) => wins[index] = Value::from(position),
            None => wins.push(Value::from(position)),
        }
        crate::json::add_to(ctx.private_state(), "achievement_points", PVP_WIN_POINTS);
    }

    apply_kills(ctx.data, &kills_log);

    let victories = get(ctx.map_ref(), "universAttackWin").as_array().map(Vec::len).unwrap_or(0);
    if won && victories >= LEAGUE_WIN_THRESHOLD {
        let level = int_or(get(ctx.info_ref(), "pvpLevel"), 1) + 1;
        ctx.info().insert("pvpLevel".into(), Value::from(level));
        crate::json::add_to(ctx.private_state(), "achievement_points", LEAGUE_COMPLETE_POINTS);
        ctx.map().insert("universAttackWin".into(), Value::Array(Vec::new()));
        ctx.result = Value::Bool(true);
    } else {
        ctx.result = Value::Null;
    }
    Ok(())
}
