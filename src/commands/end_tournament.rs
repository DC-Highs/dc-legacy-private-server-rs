use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::enums::Resource;
use crate::game::combat::{apply_kills, parse_kills_log};
use crate::json::{get, int_or};
use crate::util::now_ts;

const TOURNAMENT_ACHIEVEMENT_POINTS: i64 = 50;

const MIN_TOURNAMENT_SECONDS: i64 = 5;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let won = ctx.flag_arg(0, "won")? == 1;
    let kills_log = match ctx.args.get(1) {
        Some(raw) => parse_kills_log(raw)?,
        None => Default::default(),
    };

    let now = now_ts();
    let begin = int_or(get(ctx.private_ref(), "beginTournamentTs"), 0);
    let end = int_or(get(ctx.private_ref(), "endTournamentTs"), 0);
    if begin <= end || now - begin < MIN_TOURNAMENT_SECONDS {
        return reject("there is no tournament in progress");
    }
    ctx.private_state().insert("endTournamentTs".into(), Value::from(now));

    apply_kills(ctx.data, &kills_log);
    if !won {
        return Ok(());
    }

    let current = int_or(get(ctx.private_ref(), "currentTournament"), 1);
    let Some(tournament) = ctx.config.section("tournaments").get(current.to_string()) else {
        tracing::warn!("Tournament {current} does not exist in the game config");
        return Ok(());
    };
    let rewards = get(get(tournament, "reward"), "resources");

    for (code, resource) in [("g", Resource::Gold), ("f", Resource::Food), ("c", Resource::Cash)] {
        let amount = int_or(get(rewards, code), 0);
        if amount > 0 {
            ctx.add_resource(resource, amount);
        }
    }
    ctx.private_state().insert("currentTournament".into(), Value::from(current + 1));
    crate::json::add_to(ctx.private_state(), "achievement_points", TOURNAMENT_ACHIEVEMENT_POINTS);
    Ok(())
}
