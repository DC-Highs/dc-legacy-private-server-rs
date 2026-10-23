//! Administration API: manage accounts and their game saves.
use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{check_len, check_range, Body};
use crate::auth::{service, Admin};
use crate::config::DEUS_PLAYER_ID;
use crate::db::User;
use crate::error::{conflict_with, forbidden, not_found, validation_error, ApiError, ApiResult, StoreError};
use crate::game::pvp::generate_pvp_opponents;
use crate::state::AppState;
use crate::storage::default_player_name;
use crate::{friends, game_versions};

const MAX_PAGE_SIZE: i64 = 100;
const MAX_RESOURCE: i64 = 2_000_000_000;
const MAX_PLAYER_ID: i64 = 1 << 53;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateUserBody {
    email: String,
    password: String,
    #[serde(rename = "isAdmin", default)]
    is_admin: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateUserBody {
    email: Option<String>,
    password: Option<String>,
    #[serde(rename = "isAdmin")]
    is_admin: Option<bool>,
    #[serde(rename = "isActive")]
    is_active: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddFriendBody {
    #[serde(rename = "friendId")]
    friend_id: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GameVersionBody {
    file: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourcesBody {
    gold: Option<i64>,
    cash: Option<i64>,
    food: Option<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/users", get(list_users).post(create_user))
        .route("/api/admin/users/{user_id}", get(get_user).patch(update_user).delete(delete_user))
        .route("/api/admin/users/{user_id}/reset-player", post(reset_player))
        .route("/api/admin/users/{user_id}/resources", patch(set_resources))
        .route("/api/admin/pvp/generate", post(generate_pvp))
        .route("/api/admin/users/{user_id}/friends", get(list_friends).post(add_friend))
        .route("/api/admin/users/{user_id}/friends/{friend_id}", delete(remove_friend))
        .route("/api/admin/game-version", get(get_game_version).put(set_game_version))
}

async fn find_user(state: &AppState, user_id: i64) -> ApiResult<User> {
    state.accounts.get_user(user_id).await?.ok_or_else(|| not_found("User not found"))
}

async fn find_editable_user(state: &AppState, user_id: i64) -> ApiResult<User> {
    if user_id == DEUS_PLAYER_ID {
        return Err(forbidden("This is a system account"));
    }
    find_user(state, user_id).await
}

async fn user_with_player(state: &AppState, user: &User) -> ApiResult<Value> {
    let summary = state.storage.players_summaries(&[user.id]).await?.remove(&user.id);
    Ok(service::user_to_json(user, summary.map(|s| s.to_json())))
}

async fn list_users(
    State(state): State<AppState>,
    _admin: Admin,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let query: String = params.get("query").map(|q| q.trim().chars().take(100).collect()).unwrap_or_default();
    let number = |name: &str, default: i64| -> ApiResult<i64> {
        match params.get(name) {
            None => Ok(default),
            Some(raw) => raw.trim().parse().map_err(|_| validation_error(&[("page", "page and pageSize must be integers")])),
        }
    };
    let page = number("page", 1)?.max(1);
    let page_size = number("pageSize", 20)?.clamp(1, MAX_PAGE_SIZE);

    let (users, total) = state.accounts.list_users(&query, (page - 1) * page_size, page_size).await?;
    let ids: Vec<i64> = users.iter().map(|u| u.id).collect();
    let summaries = state.storage.players_summaries(&ids).await?;
    let items: Vec<Value> =
        users.iter().map(|u| service::user_to_json(u, summaries.get(&u.id).map(|s| s.to_json()))).collect();
    Ok(Json(json!({ "items": items, "total": total, "page": page, "pageSize": page_size })))
}

async fn create_user(State(state): State<AppState>, Admin(me): Admin, Body(body): Body<CreateUserBody>) -> ApiResult<Response> {
    check_len("email", &body.email, 254)?;
    check_len("password", &body.password, 256)?;
    let user = service::create_user(&state, &body.email, &body.password, body.is_admin).await?;
    tracing::info!("Admin {} created account {}", me.id, user.id);
    Ok((StatusCode::CREATED, Json(json!({ "user": user_with_player(&state, &user).await? }))).into_response())
}

async fn get_user(State(state): State<AppState>, _admin: Admin, Path(user_id): Path<i64>) -> ApiResult<Json<Value>> {
    let user = find_user(&state, user_id).await?;
    Ok(Json(json!({ "user": user_with_player(&state, &user).await? })))
}

async fn update_user(
    State(state): State<AppState>,
    Admin(me): Admin,
    Path(user_id): Path<i64>,
    Body(body): Body<UpdateUserBody>,
) -> ApiResult<Json<Value>> {
    if let Some(email) = &body.email {
        check_len("email", email, 254)?;
    }
    if let Some(password) = &body.password {
        check_len("password", password, 256)?;
    }
    let mut user = find_editable_user(&state, user_id).await?;

    if user.id == me.id && (body.is_active == Some(false) || body.is_admin == Some(false)) {
        return Err(forbidden("You cannot disable or demote your own account"));
    }

    if let Some(email) = &body.email {
        let email = service::normalize_email(email)?;
        if let Some(taken) = state.accounts.get_user_by_email(&email).await? {
            if taken.id != user.id {
                return Err(conflict_with("This email is already registered", &[("email", "Already registered")]));
            }
        }
        user.email = email;
    }
    let mut end_sessions = false;
    if let Some(password) = &body.password {
        service::check_password(password, &user.email)?;
        user.password_hash = crate::auth::passwords::hash_password_async(password.clone()).await;
        end_sessions = true;
    }
    if let Some(is_admin) = body.is_admin {
        user.is_admin = is_admin;
    }
    if let Some(is_active) = body.is_active {
        user.is_active = is_active;
        end_sessions |= !is_active;
    }

    match state.accounts.update_user(&user).await {
        Ok(()) => {}
        Err(StoreError::Conflict) => {
            return Err(ApiError::new(409, "conflict", "This email is already registered")
                .with_fields([("email".to_string(), "Already registered".to_string())].into()))
        }
        Err(other) => return Err(other.into()),
    }
    if end_sessions {
        service::end_all_sessions(&state, user.id).await?;
    }
    service::forget_user(&state, user.id);

    tracing::info!("Admin {} updated account {user_id}", me.id);
    Ok(Json(json!({ "user": user_with_player(&state, &user).await? })))
}

async fn delete_user(State(state): State<AppState>, Admin(me): Admin, Path(user_id): Path<i64>) -> ApiResult<Json<Value>> {
    if user_id == me.id {
        return Err(forbidden("You cannot delete your own account"));
    }
    let user = find_editable_user(&state, user_id).await?;
    service::forget_user(&state, user.id);
    state.accounts.delete_user(user.id).await?;
    state.storage.delete_player(user.id).await?;
    tracing::info!("Admin {} deleted account {user_id}", me.id);
    Ok(Json(json!({ "ok": true })))
}

async fn reset_player(State(state): State<AppState>, Admin(me): Admin, Path(user_id): Path<i64>) -> ApiResult<Json<Value>> {
    let user = find_user(&state, user_id).await?;
    {
        let _guard = state.storage.lock(user.id).await;
        state.storage.create_player(user.id, &default_player_name(&user.email), true).await?;
    }
    tracing::info!("Admin {} reset the game save of account {user_id}", me.id);
    Ok(Json(json!({ "user": user_with_player(&state, &user).await? })))
}

async fn set_resources(
    State(state): State<AppState>,
    Admin(me): Admin,
    Path(user_id): Path<i64>,
    Body(body): Body<ResourcesBody>,
) -> ApiResult<Json<Value>> {
    let changes: Vec<(&str, i64)> = [("gold", body.gold), ("cash", body.cash), ("food", body.food)]
        .into_iter()
        .filter_map(|(name, value)| value.map(|v| (name, v)))
        .collect();
    for (name, value) in &changes {
        check_range(name, *value, 0, MAX_RESOURCE)?;
    }
    if changes.is_empty() {
        return Err(validation_error(&[("_", "Provide at least one of gold, cash, food")]));
    }

    let user = find_user(&state, user_id).await?;
    {
        let _guard = state.storage.lock(user.id).await;
        let Some(mut loaded) = state.storage.load_player(user.id).await? else {
            return Err(not_found("Game save not found"));
        };
        if let Some(info) = loaded.data.get_mut("playerInfo").and_then(Value::as_object_mut) {
            for (name, value) in &changes {
                info.insert(name.to_string(), Value::from(*value));
            }
        }
        state.storage.save_player(user.id, &loaded.data).await?;
    }
    tracing::info!("Admin {} set resources {changes:?} of account {user_id}", me.id);
    Ok(Json(json!({ "user": user_with_player(&state, &user).await? })))
}

async fn generate_pvp(State(state): State<AppState>, _admin: Admin) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "totalOpponents": generate_pvp_opponents(&state).await? })))
}

async fn friend_entries(state: &AppState, user_id: i64) -> ApiResult<Vec<Value>> {
    let friends = friends::friends_of(state, user_id).await?;
    let ids: Vec<i64> = friends.iter().map(|u| u.id).collect();
    let summaries = state.storage.players_summaries(&ids).await?;
    Ok(friends
        .iter()
        .map(|u| {
            let name = summaries.get(&u.id).map(|s| s.name.clone()).unwrap_or_default();
            json!({ "id": u.id, "email": u.email, "playerName": name })
        })
        .collect())
}

async fn list_friends(State(state): State<AppState>, _admin: Admin, Path(user_id): Path<i64>) -> ApiResult<Json<Value>> {
    find_user(&state, user_id).await?;
    Ok(Json(json!({ "items": friend_entries(&state, user_id).await? })))
}

async fn add_friend(
    State(state): State<AppState>,
    Admin(me): Admin,
    Path(user_id): Path<i64>,
    Body(body): Body<AddFriendBody>,
) -> ApiResult<Response> {
    check_range("friendId", body.friend_id, 1, MAX_PLAYER_ID)?;
    find_user(&state, user_id).await?;
    friends::add_friends(&state, user_id, body.friend_id).await?;
    let items = friend_entries(&state, user_id).await?;
    friends::award_both(&state, user_id, body.friend_id).await?;
    tracing::info!("Admin {} made {user_id} and {} friends", me.id, body.friend_id);
    Ok((StatusCode::CREATED, Json(json!({ "items": items }))).into_response())
}

async fn remove_friend(
    State(state): State<AppState>,
    Admin(me): Admin,
    Path((user_id, friend_id)): Path<(i64, i64)>,
) -> ApiResult<Json<Value>> {
    find_user(&state, user_id).await?;
    if !friends::remove_friends(&state, user_id, friend_id).await? {
        return Err(not_found("They are not friends"));
    }
    tracing::info!("Admin {} ended the friendship of {user_id} and {friend_id}", me.id);
    Ok(Json(json!({ "items": friend_entries(&state, user_id).await? })))
}

async fn get_game_version(State(state): State<AppState>, _admin: Admin) -> ApiResult<Json<Value>> {
    Ok(Json(game_versions::payload(&state).await?))
}

async fn set_game_version(State(state): State<AppState>, Admin(me): Admin, Body(body): Body<GameVersionBody>) -> ApiResult<Json<Value>> {
    check_len("file", &body.file, 255)?;
    if !game_versions::select_version(&state, &body.file).await? {
        return Err(validation_error(&[("file", "This version is not in flash/versions")]));
    }
    tracing::info!("Admin {} selected the game version {}", me.id, body.file);
    Ok(Json(game_versions::payload(&state).await?))
}
