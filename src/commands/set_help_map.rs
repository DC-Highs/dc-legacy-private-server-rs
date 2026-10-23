use serde_json::Value;
use super::ctx::Ctx;
use super::validation::{reject, CmdResult};
use crate::json::{ensure_array, id_text};

const MAX_HELP_ENTRIES: usize = 200;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let help_id = id_text(ctx.arg(0, "help id")?);
    if help_id.chars().count() > 32 {
        return reject("help id is too long");
    }
    let help_map = ensure_array(ctx.private_state(), "helpMap");
    if help_map.iter().any(|h| h.as_str() == Some(help_id.as_str())) {
        return reject(format!("the player had already seen help '{help_id}'"));
    }
    if help_map.len() >= MAX_HELP_ENTRIES {
        return reject("too many help entries");
    }
    help_map.push(Value::from(help_id));
    Ok(())
}
