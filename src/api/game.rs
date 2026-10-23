//! Data the web client needs to start the Flash game, and the liveness probe.
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::auth::Login;
use crate::config::LEGACY_GAME_HOSTS;
use crate::error::ApiResult;
use crate::state::AppState;
use crate::{friends, game_versions};

const SERVICES_PATH: &str = "/static/dragoncity/";

pub fn router() -> Router<AppState> {
    Router::new().route("/api/game/bootstrap", get(bootstrap)).route("/api/game/health", get(health))
}

async fn bootstrap(State(state): State<AppState>, Login(user): Login) -> ApiResult<Json<Value>> {
    let player_name = state
        .storage
        .peek_player(user.id)
        .await?
        .and_then(|data| data.pointer("/playerInfo/name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let friends_flashvar = friends::friends_flashvar(&state, user.id).await?;
    let swf_version = game_versions::selected_version(&state).await?;

    Ok(Json(json!({
        "playerId": user.id.to_string(),
        "playerName": player_name,
        "loader": "flash/DCLoader.swf",
        "servicesPath": SERVICES_PATH,
        "flashVars": {
            "swftoload": format!("flash/versions/{swf_version}"),
            "use_assets_opt": true,
            "fb_sig_user": user.id.to_string(),
            "language": "en",
            "skiphash12341": "",
            "user_key": "12304531",
            "getText": "",
            "push_time": "1",
            "offer_type": "1",
            "fb_promo_uri": "",
            "spdebug": "0",
            "fb_sig_friends": friends_flashvar,
        },
        "legacyHosts": LEGACY_GAME_HOSTS,
    })))
}

async fn health(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    state.accounts.ping().await?;
    state.storage.store().ping().await?;
    Ok(Json(json!({ "status": "ok" })))
}
