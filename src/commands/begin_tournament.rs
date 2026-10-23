use serde_json::Value;
use super::ctx::Ctx;
use super::validation::CmdResult;
use crate::util::now_ts;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    ctx.private_state().insert("beginTournamentTs".into(), Value::from(now_ts()));
    Ok(())
}
