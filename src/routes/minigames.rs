use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{check_range, Body};
use crate::auth::Login;
use crate::error::{not_found, too_many_requests, ApiResult};
use crate::json::{add_to, ensure_object};
use crate::state::AppState;
use crate::util::now_ts;

const MAX_GEMS_PER_GAME: i64 = 3;
const COOLDOWN_SECONDS: i64 = 60;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RewardBody {
    gems: i64,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/minigame/reward", post(minigame_reward))
}

async fn minigame_reward(State(state): State<AppState>, Login(user): Login, Body(body): Body<RewardBody>) -> ApiResult<Json<Value>> {
    check_range("gems", body.gems, 0, MAX_GEMS_PER_GAME)?;
    if body.gems == 0 {
        return Ok(Json(json!({ "status": "ok", "gems_earned": 0, "new_cash": null })));
    }

    let now = now_ts();
    let _guard = state.storage.lock(user.id).await;
    if now - state.storage.get_last_minigame_ts(user.id).await? < COOLDOWN_SECONDS {
        return Err(too_many_requests("Wait a moment before playing again"));
    }
    let Some(mut loaded) = state.storage.load_player(user.id).await? else {
        return Err(not_found("Player not found"));
    };

    let root = loaded.data.as_object_mut().ok_or_else(crate::error::internal)?;
    let new_cash = add_to(ensure_object(root, "playerInfo"), "cash", body.gems);
    state.storage.save_player(user.id, &loaded.data).await?;
    state.storage.set_last_minigame_ts(user.id, now).await?;

    tracing::info!("Minigame: player {} earned {} gem(s) -> {new_cash}", user.id, body.gems);
    Ok(Json(json!({ "status": "ok", "gems_earned": body.gems, "new_cash": new_cash })))
}
