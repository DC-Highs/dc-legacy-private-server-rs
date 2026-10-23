//! Signed JSON endpoints under `/static/dragoncity/srv`, tracking stubs and the SSE stream.
use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream;
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;

use super::packet::form_value;
use crate::auth::Login;
use crate::config::SSE_KEEPALIVE_SECONDS;
use crate::enums::sse;
use crate::error::{not_found, validation_error, ApiResult};
use crate::game::black_market::check_and_update_black_market;
use crate::game::pvp::pvp_page;
use crate::security::sign_packet;
use crate::state::AppState;
use crate::friends;

const MAX_PVP_LEVEL: i64 = 1000;

const GAME_CONFIG_FILES: [&str; 2] = ["get_game_config.json", "get_game_config.php"];
const PVP_ISLE_FILES: [&str; 3] = ["get_pvp_isle.json", "get_pvp_isle.php", "get_pvp_isle_info.php"];
const PLAYER_INFO_FILES: [&str; 2] = ["get_player_info.json", "get_player_info.php"];

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/static/dragoncity/srv/sync_error_track.php", get(empty_ok).post(empty_ok))
        .route("/static/dragoncity/srv/track_game_status.php", get(empty_ok).post(empty_ok))
        .route("/static/dragoncity/srv/{filename}", get(srv_handler).post(srv_handler))
        .route("/static/dragoncity/deity_world/track/open", post(track_ok))
        .route("/static/dragoncity/deity_world/track/watch-tutorial", post(track_ok))
        .route("/stream", get(stream_events))
}

async fn empty_ok() -> StatusCode {
    StatusCode::OK
}

async fn track_ok() -> Json<Value> {
    Json(json!({ "result": true }))
}

fn text_response(body: impl Into<axum::body::Body>, content_type: &'static str, no_cache: bool) -> Response {
    let mut response = Response::new(body.into());
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if no_cache {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store, no-cache, must-revalidate"));
        headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    }
    response
}

fn visited_player_id(own_id: i64, body: &[u8], query: &HashMap<String, String>) -> ApiResult<Option<i64>> {
    let raw = form_value(body, "user").filter(|v| !v.is_empty()).or_else(|| query.get("user").cloned());
    match raw.as_deref() {
        None | Some("") | Some("0") => Ok(None),
        Some(text) => {
            let target: i64 =
                text.trim().parse().map_err(|_| validation_error(&[("user", "user must be an integer")]))?;
            Ok((target != own_id).then_some(target))
        }
    }
}

async fn pvp_response(state: &AppState, level_arg: &str) -> ApiResult<Response> {
    let level: i64 = level_arg.trim().parse().map_err(|_| validation_error(&[("level", "level must be an integer")]))?;
    let level = level.clamp(1, MAX_PVP_LEVEL);
    let page = pvp_page(state, level).await?;
    tracing::debug!("PvP league {level} -> {} opponents", page.len());
    Ok(text_response(sign_packet(&page), "text/plain; charset=utf-8", true))
}

async fn own_player_info(state: &AppState, player_id: i64) -> ApiResult<Value> {
    let mut data = {
        let _guard = state.storage.lock(player_id).await;
        let Some(mut loaded) = state.storage.load_player(player_id).await? else {
            return Err(not_found("Game save not found"));
        };

        let before = loaded.data.clone();
        if let Some(info) = loaded.data.get_mut("playerInfo").and_then(Value::as_object_mut) {
            let has_pid = info.get("pid").map(crate::json::truthy).unwrap_or(false);
            if !has_pid {
                info.insert("pid".into(), Value::from(player_id.to_string()));
            }
        }
        if let Some(offers) = check_and_update_black_market(&mut loaded.data, &loaded.config, player_id) {
            state.storage.save_kompu_offers(player_id, &offers).await?;
        }
        if loaded.merged_assists || loaded.data != before {
            state.storage.save_player(player_id, &loaded.data).await?;
        }
        loaded.data
    };

    let neighbors = friends::neighbors_payload(state, player_id).await?;
    if let Some(root) = data.as_object_mut() {
        root.insert("neighbors".into(), Value::Array(neighbors));
    }
    Ok(data)
}

async fn srv_handler(
    State(state): State<AppState>,
    Login(user): Login,
    Path(filename): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    body: Bytes,
) -> ApiResult<Response> {
    let player_id = user.id;
    let target_id = visited_player_id(player_id, &body, &query)?;
    tracing::debug!("SRV request {filename} | player: {player_id} | visiting: {target_id:?}");
    let name = filename.as_str();

    if GAME_CONFIG_FILES.contains(&name) {
        let config = state.storage.config_for_player(player_id).await?;
        let signed = state.storage.game.signed_config(&config);
        return Ok(text_response(signed, "text/plain; charset=utf-8", true));
    }

    if PVP_ISLE_FILES.contains(&name) {
        return pvp_response(&state, query.get("level").map(String::as_str).unwrap_or("1")).await;
    }

    if PLAYER_INFO_FILES.contains(&name) {
        let data = match target_id {
            None => own_player_info(&state, player_id).await?,
            Some(target) => {
                friends::require_friendship(&state, player_id, target).await?;
                let Some(mut data) = state.storage.peek_player(target).await? else {
                    return Err(not_found("Neighbour not found"));
                };
                if let Some(info) = data.get_mut("playerInfo").and_then(Value::as_object_mut) {
                    if !info.get("pid").map(crate::json::truthy).unwrap_or(false) {
                        info.insert("pid".into(), Value::from(target.to_string()));
                    }
                }
                data
            }
        };
        return Ok(text_response(sign_packet(&data), "text/plain; charset=utf-8", false));
    }

    Ok(Json(json!({ "result": "200" })).into_response())
}

struct StreamGuard {
    state: AppState,
    player_id: i64,
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        self.state.sse.release(self.player_id);
    }
}

async fn stream_events(State(state): State<AppState>, Login(user): Login) -> Response {
    let receiver = state.sse.subscribe(user.id);
    let guard = StreamGuard { state, player_id: user.id };
    tracing::debug!("SSE channel opened for player {}", user.id);

    let events = stream::unfold((receiver, guard), |(mut receiver, guard)| async move {
        let message = match tokio::time::timeout(Duration::from_secs(SSE_KEEPALIVE_SECONDS), receiver.recv()).await {
            Ok(Ok(message)) => message,
            Ok(Err(RecvError::Lagged(_))) => sse::GAME_SAVED,
            Ok(Err(RecvError::Closed)) => return None,
            Err(_) => sse::PING,
        };
        Some((Ok::<Event, Infallible>(Event::default().data(message)), (receiver, guard)))
    });

    let mut response = Sse::new(events).into_response();
    response.headers_mut().insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}
