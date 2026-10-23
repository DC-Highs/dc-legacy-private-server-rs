use serde_json::Value;
use super::ctx::Ctx;
use super::validation::CmdResult;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let helper = ctx.int_arg_min(0, "helper id", 1)?;
    if let Some(received) = ctx.map().get_mut("receivedAssists").and_then(Value::as_object_mut) {
        received.remove(&format!("u{helper}"));
    }
    Ok(())
}
