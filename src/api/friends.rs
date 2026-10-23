//! Friend requests between players (the friends become neighbours in the game).
use std::collections::HashMap;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{check_range, Body};
use crate::auth::{service::iso, Login};
use crate::error::{not_found, ApiResult};
use crate::state::AppState;
use crate::friends;

const HOUR: Duration = Duration::from_secs(3600);
const MINUTE: Duration = Duration::from_secs(60);
const MAX_PLAYER_ID: i64 = 1 << 53;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestBody {
    #[serde(rename = "playerId")]
    player_id: i64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/friends", get(overview))
        .route("/api/friends/summary", get(summary))
        .route("/api/friends/requests", post(send_request))
        .route("/api/friends/requests/{request_id}/accept", post(accept))
        .route("/api/friends/requests/{request_id}/decline", post(decline))
        .route("/api/friends/requests/{request_id}", delete(cancel))
        .route("/api/friends/{player_id}", delete(remove))
}

async fn people(state: &AppState, ids: &[i64]) -> ApiResult<HashMap<i64, Value>> {
    let summaries = state.storage.players_summaries(ids).await?;
    Ok(ids
        .iter()
        .map(|id| {
            let (name, level) = summaries.get(id).map(|s| (s.name.clone(), s.level)).unwrap_or((String::new(), 1));
            (*id, json!({ "id": id, "playerName": name, "level": level }))
        })
        .collect())
}

async fn overview(State(state): State<AppState>, Login(user): Login) -> ApiResult<Json<Value>> {
    let friend_users = friends::friends_of(&state, user.id).await?;
    let (incoming, outgoing) = friends::pending_requests(&state, user.id).await?;

    let mut ids: Vec<i64> = friend_users.iter().map(|u| u.id).collect();
    ids.extend(incoming.iter().map(|r| r.requester_id));
    ids.extend(outgoing.iter().map(friends::addressee_id));
    let profiles = people(&state, &ids).await?;
    let profile = |id: i64| profiles.get(&id).cloned().unwrap_or(Value::Null);

    Ok(Json(json!({
        "playerId": user.id,
        "friends": friend_users.iter().map(|u| profile(u.id)).collect::<Vec<_>>(),
        "incoming": incoming.iter().map(|r| json!({
            "requestId": r.id, "createdAt": iso(r.created_at), "from": profile(r.requester_id),
        })).collect::<Vec<_>>(),
        "outgoing": outgoing.iter().map(|r| json!({
            "requestId": r.id, "createdAt": iso(r.created_at), "to": profile(friends::addressee_id(r)),
        })).collect::<Vec<_>>(),
        "limits": {
            "maxFriends": state.settings.max_friends,
            "maxPendingRequests": state.settings.max_pending_requests,
            "rewardGems": friends::FRIEND_REWARD_GEMS,
            "rewardLimit": friends::FRIEND_REWARD_LIMIT,
        },
    })))
}

async fn summary(State(state): State<AppState>, Login(user): Login) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "incoming": friends::count_incoming(&state, user.id).await? })))
}

async fn send_request(State(state): State<AppState>, Login(user): Login, Body(body): Body<RequestBody>) -> ApiResult<Response> {
    state.rate_limit(&format!("friend-request:{}", user.id), &[(30, HOUR), (10, MINUTE)])?;
    check_range("playerId", body.player_id, 1, MAX_PLAYER_ID)?;

    let (row, accepted) = friends::send_request(&state, user.id, body.player_id).await?;
    let gems = if accepted { friends::award_both(&state, user.id, body.player_id).await?.0 } else { 0 };
    tracing::info!("Player {} asked {} to be friends (accepted at once: {accepted})", user.id, body.player_id);
    Ok((StatusCode::CREATED, Json(json!({ "requestId": row.id, "accepted": accepted, "gemsAwarded": gems }))).into_response())
}

async fn accept(State(state): State<AppState>, Login(user): Login, Path(request_id): Path<i64>) -> ApiResult<Json<Value>> {
    let row = friends::accept_request(&state, user.id, request_id).await?;
    let gems = friends::award_both(&state, user.id, row.requester_id).await?.0;
    tracing::info!("Player {} accepted the friend request of {}", user.id, row.requester_id);
    Ok(Json(json!({ "ok": true, "gemsAwarded": gems })))
}

async fn decline(State(state): State<AppState>, Login(user): Login, Path(request_id): Path<i64>) -> ApiResult<Json<Value>> {
    friends::decline_request(&state, user.id, request_id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn cancel(State(state): State<AppState>, Login(user): Login, Path(request_id): Path<i64>) -> ApiResult<Json<Value>> {
    friends::cancel_request(&state, user.id, request_id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn remove(State(state): State<AppState>, Login(user): Login, Path(player_id): Path<i64>) -> ApiResult<Json<Value>> {
    if !friends::remove_friends(&state, user.id, player_id).await? {
        return Err(not_found("They are not your friend"));
    }
    tracing::info!("Player {} removed {player_id} from their friends", user.id);
    Ok(Json(json!({ "ok": true })))
}
