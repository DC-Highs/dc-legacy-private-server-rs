use super::ctx::Ctx;
use super::validation::{check_json_depth, parse_json_text, reject, CmdResult};
use crate::friends;

const MAX_CLICKS_PER_ASSIST: usize = 50;

pub async fn run(ctx: &mut Ctx<'_>) -> CmdResult<()> {
    let target_id = ctx.int_arg_min(0, "neighbor id", 1)?;
    let raw_clicks = ctx.text_arg(1, "clicks", 256, true)?;

    if target_id == ctx.user_id {
        return reject("a player cannot assist themselves");
    }

    if !friends::are_friends(ctx.state, ctx.user_id, target_id).await? {
        return reject(format!("{target_id} is not one of the player's neighbours"));
    }

    if !ctx.state.storage.player_exists(target_id).await? {
        return reject(format!("neighbor {target_id} does not exist"));
    }

    let clicks = parse_json_text(&raw_clicks, "clicks")?;
    
    let Some(list) = clicks.as_array().filter(|c| !c.is_empty() && c.len() <= MAX_CLICKS_PER_ASSIST) else {
        return reject(format!("clicks must be a list of 1 to {MAX_CLICKS_PER_ASSIST} entries"));
    };

    check_json_depth(&clicks, "clicks", 2, MAX_CLICKS_PER_ASSIST * 4)?;

    ctx.state.storage.add_pending_assist(target_id, ctx.user_id, list).await?;
    Ok(())
}
