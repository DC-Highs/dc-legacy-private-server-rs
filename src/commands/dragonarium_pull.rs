use serde_json::Value;
use super::ctx::{set_field, Ctx};
use super::validation::{reject, CmdResult};
use crate::game::dragonarium::ensure_tabs;
use crate::json::ensure_array;
use super::shared::dragons::*;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let tab_index = ctx.int_arg(1, "tab", 0, 1000)? as usize;
    let position = ctx.int_arg(2, "position", 0, 100_000)? as usize;
    let habitat = ctx.int_arg_min(3, "habitat", 1)?;

    require_habitat(ctx, &habitat.to_string())?;

    let tabs = ensure_tabs(ctx.private_state());
    let Some(tab) = tabs.get_mut(tab_index).and_then(Value::as_array_mut) else {
        return reject(format!("tab index {tab_index} does not exist in the dragonarium"));
    };
    if position >= tab.len() {
        return reject(format!("dragon index {position} is out of range for tab {tab_index}"));
    }
    let mut dragon = tab.remove(position);
    set_field(&mut dragon, "habitatId", habitat)?;
    ensure_array(ctx.map(), "dragons").push(dragon);
    Ok(())
}
