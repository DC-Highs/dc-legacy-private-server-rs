use std::time::Duration;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{check_len, Body};
use crate::auth::{clear_session_cookie, service, session_cookie, ClientInfo, Identity, Login};
use crate::db::User;
use crate::error::ApiResult;
use crate::state::AppState;

const HOUR: Duration = Duration::from_secs(3600);
const MINUTE: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegisterBody {
    email: String,
    password: String,
    #[serde(rename = "confirmPassword")]
    confirm_password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginBody {
    email: String,
    password: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
}

pub async fn me_payload(state: &AppState, user: &User) -> ApiResult<Value> {
    let mut payload = service::user_to_json(user, None);
    let name = state
        .storage
        .peek_player(user.id)
        .await?
        .and_then(|data| data.pointer("/playerInfo/name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    payload["playerName"] = Value::from(name);
    Ok(payload)
}

async fn register(State(state): State<AppState>, client: ClientInfo, Body(body): Body<RegisterBody>) -> ApiResult<Response> {
    state.rate_limit(&format!("register:{}", client.ip), &[(10, HOUR), (3, MINUTE)])?;
    check_len("email", &body.email, 254)?;
    check_len("password", &body.password, 256)?;
    check_len("confirmPassword", &body.confirm_password, 256)?;

    let user = service::register_user(&state, &body.email, &body.password, &body.confirm_password).await?;
    let token = service::start_session(&state, &user, Some(&client.ip), client.user_agent.as_deref()).await?;
    let payload = json!({ "user": me_payload(&state, &user).await? });
    Ok((StatusCode::CREATED, [(header::SET_COOKIE, session_cookie(&token, &state.settings))], Json(payload)).into_response())
}

async fn login(State(state): State<AppState>, client: ClientInfo, Body(body): Body<LoginBody>) -> ApiResult<Response> {
    state.rate_limit(&format!("login:{}", client.ip), &[(20, HOUR), (5, MINUTE)])?;
    check_len("email", &body.email, 254)?;
    check_len("password", &body.password, 256)?;

    let user = service::authenticate(&state, &body.email, &body.password).await?;
    service::ensure_player(&state, &user).await?;
    let token = service::start_session(&state, &user, Some(&client.ip), client.user_agent.as_deref()).await?;
    let payload = json!({ "user": me_payload(&state, &user).await? });
    tracing::info!("Account {} logged in", user.id);
    Ok(([(header::SET_COOKIE, session_cookie(&token, &state.settings))], Json(payload)).into_response())
}

async fn logout(State(state): State<AppState>, identity: Identity) -> ApiResult<Response> {
    if let Some(token) = &identity.token {
        service::end_session(&state, token).await?;
    }
    Ok(([(header::SET_COOKIE, clear_session_cookie(&state.settings))], Json(json!({ "ok": true }))).into_response())
}

async fn me(State(state): State<AppState>, Login(user): Login) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "user": me_payload(&state, &user).await? })))
}
