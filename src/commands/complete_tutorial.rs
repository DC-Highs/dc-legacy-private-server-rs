use serde_json::Value;
use super::ctx::Ctx;
use super::validation::CmdResult;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let step = ctx.int_arg(0, "tutorial step", 0, 1000)?;
    ctx.info().insert("completed_tutorial".into(), Value::from(step.to_string()));
    Ok(())
}
