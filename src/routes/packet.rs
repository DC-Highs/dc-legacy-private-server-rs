//! The `packet.php` endpoint: applies batched client commands to the logged-in player's save.
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderValue, Method};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde_json::{json, Value};

use crate::auth::Login;
use crate::commands::dispatch_commands;
use crate::enums::sse;
use crate::error::{not_found, validation_error, ApiError, ApiResult};
use crate::security::sign_packet;
use crate::state::AppState;
use crate::util::now_ts;

const MAX_PACKET_BYTES: usize = 256 * 1024;

pub fn router() -> Router<AppState> {
    Router::new().route("/static/dragoncity/srv/packet.php", get(packet_handler).post(packet_handler))
}

pub fn form_value(body: &[u8], name: &str) -> Option<String> {
    form_urlencoded::parse(body).find(|(key, _)| key == name).map(|(_, value)| value.into_owned())
}

fn read_payload(body: &[u8]) -> ApiResult<Value> {
    let raw = form_value(body, "data").unwrap_or_default();
    if raw.len() > MAX_PACKET_BYTES {
        return Err(ApiError::new(413, "payload_too_large", "Packet too large"));
    }
    let json = raw.split_once(';').map(|(_, rest)| rest).unwrap_or(&raw);
    match serde_json::from_str::<Value>(json) {
        Ok(payload) if payload.is_object() => Ok(payload),
        Ok(_) => Err(validation_error(&[("data", "The packet must be a JSON object")])),
        Err(_) => Err(validation_error(&[("data", "The packet is not valid JSON")])),
    }
}

async fn process_post(state: &AppState, player_id: i64, body: &[u8]) -> ApiResult<Vec<Value>> {
    let payload = read_payload(body)?;
    let commands = payload.get("commands").cloned().unwrap_or_else(|| json!([]));

    let _guard = state.storage.lock(player_id).await;
    let Some(mut loaded) = state.storage.load_player(player_id).await? else {
        return Err(not_found("Game save not found"));
    };

    let before = loaded.data.clone();
    let outcome = dispatch_commands(state, player_id, &mut loaded.data, &loaded.config, &commands).await;
    if loaded.merged_assists || loaded.data != before {
        state.storage.save_player(player_id, &loaded.data).await?;
    }
    drop(_guard);

    if outcome.applied_any {
        let name = loaded.data.pointer("/playerInfo/name").and_then(Value::as_str).unwrap_or("");
        tracing::debug!("Game saved for player {player_id} ({name})");
    } else {
        tracing::info!("No changes applied, none of the received commands is implemented: {:?}", outcome.ignored);
    }
    state.sse.notify(player_id, sse::GAME_SAVED);
    Ok(outcome.responses)
}

async fn packet_handler(State(state): State<AppState>, Login(user): Login, method: Method, body: Bytes) -> ApiResult<Response> {
    state.rate_limit(&format!("packet:{}", user.id), &[(600, Duration::from_secs(60))])?;

    let responses = if method == Method::POST { process_post(&state, user.id, &body).await? } else { Vec::new() };
    let signed = sign_packet(&json!({
        "result": true,
        "response": responses,
        "ts": now_ts(),
        "publishActions": 0,
    }));
    let mut response = signed.into_response();
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    Ok(response)
}
