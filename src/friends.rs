//! Friendships between accounts. In the game they are the player's neighbours.
use std::collections::HashMap;

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::config::DEUS_PLAYER_ID;
use crate::db::{Friendship, User, FRIEND_ACCEPTED, FRIEND_DECLINED, FRIEND_PENDING};
use crate::error::{
    conflict, not_found, too_many_requests, validation_error, ApiResult, StoreError,
};
use crate::json::{ensure_array, ensure_object, int_or};
use crate::state::AppState;

pub const DECLINE_COOLDOWN_DAYS: i64 = 7;

pub const FRIEND_REWARD_GEMS: i64 = 5;
pub const FRIEND_REWARD_LIMIT: usize = 5;

fn pair(a: i64, b: i64) -> (i64, i64) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

pub fn addressee_id(friendship: &Friendship) -> i64 {
    if friendship.requester_id == friendship.high_id {
        friendship.low_id
    } else {
        friendship.high_id
    }
}

async fn get_row(state: &AppState, a: i64, b: i64) -> ApiResult<Option<Friendship>> {
    let (low, high) = pair(a, b);
    Ok(state.accounts.get_friendship_pair(low, high).await?)
}

pub async fn friend_ids(state: &AppState, user_id: i64) -> ApiResult<Vec<i64>> {
    Ok(state.accounts.accepted_friend_ids(user_id).await?)
}

pub async fn are_friends(state: &AppState, a: i64, b: i64) -> ApiResult<bool> {
    if a == b {
        return Ok(false);
    }
    Ok(get_row(state, a, b).await?.map(|row| row.status == FRIEND_ACCEPTED).unwrap_or(false))
}

pub async fn friends_of(state: &AppState, user_id: i64) -> ApiResult<Vec<User>> {
    let ids = friend_ids(state, user_id).await?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let users: HashMap<i64, User> = state.accounts.users_by_ids(&ids).await?.into_iter().map(|u| (u.id, u)).collect();
    Ok(ids.iter().filter_map(|id| users.get(id).cloned()).collect())
}

pub async fn remove_friends(state: &AppState, a: i64, b: i64) -> ApiResult<bool> {
    match get_row(state, a, b).await? {
        Some(row) if row.status == FRIEND_ACCEPTED => {
            state.accounts.delete_friendship(row.id).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn accept(state: &AppState, friendship: &mut Friendship) -> ApiResult<()> {
    for user_id in [friendship.low_id, friendship.high_id] {
        if state.accounts.count_accepted(user_id).await? >= state.settings.max_friends {
            return Err(conflict("A friend list is full"));
        }
    }
    friendship.status = FRIEND_ACCEPTED.to_string();
    friendship.responded_at = Some(Utc::now());
    state.accounts.update_friendship(friendship).await?;
    Ok(())
}

pub async fn send_request(state: &AppState, requester_id: i64, target_id: i64) -> ApiResult<(Friendship, bool)> {
    if requester_id == target_id {
        return Err(validation_error(&[("playerId", "You cannot add yourself")]));
    }
    match state.accounts.get_user(target_id).await? {
        Some(target) if target.is_active => {}
        _ => return Err(not_found("Player not found")),
    }

    let mut existing = get_row(state, requester_id, target_id).await?;
    if let Some(row) = existing.as_mut() {
        match row.status.as_str() {
            FRIEND_ACCEPTED => return Err(conflict("You are already friends")),
            FRIEND_PENDING => {
                if row.requester_id == target_id {
                    accept(state, row).await?;
                    return Ok((row.clone(), true));
                }
                return Err(conflict("You already sent a request to this player"));
            }
            _ => {
                let recently = row
                    .responded_at
                    .map(|at| Utc::now() - at < Duration::days(DECLINE_COOLDOWN_DAYS))
                    .unwrap_or(false);
                if row.requester_id == requester_id && recently {
                    return Err(too_many_requests("This player declined your request recently. Try again later."));
                }
            }
        }
    }

    let max_pending = state.settings.max_pending_requests;
    if state.accounts.count_pending_sent(requester_id).await? >= max_pending {
        return Err(conflict(&format!(
            "You can have at most {max_pending} pending requests. Wait for an answer or cancel one."
        )));
    }
    if state.accounts.count_accepted(requester_id).await? >= state.settings.max_friends {
        return Err(conflict("Your friend list is full"));
    }

    let row = match existing {
        None => {
            let (low, high) = pair(requester_id, target_id);
            match state.accounts.insert_friendship(low, high, requester_id, FRIEND_PENDING).await {
                Ok(row) => row,
                Err(StoreError::Conflict) => return Err(conflict("You already sent a request to this player")),
                Err(other) => return Err(other.into()),
            }
        }
        Some(mut row) => {
            row.requester_id = requester_id;
            row.status = FRIEND_PENDING.to_string();
            row.created_at = Utc::now();
            row.responded_at = None;
            state.accounts.update_friendship(&row).await?;
            row
        }
    };
    Ok((row, false))
}

async fn request_for_addressee(state: &AppState, user_id: i64, request_id: i64) -> ApiResult<Friendship> {
    match state.accounts.get_friendship(request_id).await? {
        Some(row) if row.status == FRIEND_PENDING && addressee_id(&row) == user_id => Ok(row),
        _ => Err(not_found("Request not found")),
    }
}

pub async fn accept_request(state: &AppState, user_id: i64, request_id: i64) -> ApiResult<Friendship> {
    let mut row = request_for_addressee(state, user_id, request_id).await?;
    accept(state, &mut row).await?;
    Ok(row)
}

pub async fn decline_request(state: &AppState, user_id: i64, request_id: i64) -> ApiResult<()> {
    let mut row = request_for_addressee(state, user_id, request_id).await?;
    row.status = FRIEND_DECLINED.to_string();
    row.responded_at = Some(Utc::now());
    state.accounts.update_friendship(&row).await?;
    Ok(())
}

pub async fn cancel_request(state: &AppState, user_id: i64, request_id: i64) -> ApiResult<()> {
    match state.accounts.get_friendship(request_id).await? {
        Some(row) if row.status == FRIEND_PENDING && row.requester_id == user_id => {
            state.accounts.delete_friendship(row.id).await?;
            Ok(())
        }
        _ => Err(not_found("Request not found")),
    }
}

pub async fn pending_requests(state: &AppState, user_id: i64) -> ApiResult<(Vec<Friendship>, Vec<Friendship>)> {
    let rows = state.accounts.pending_involving(user_id).await?;
    let (outgoing, incoming) = rows.into_iter().partition(|row| row.requester_id == user_id);
    Ok((incoming, outgoing))
}

pub async fn count_incoming(state: &AppState, user_id: i64) -> ApiResult<i64> {
    Ok(state.accounts.count_incoming(user_id).await?)
}

pub async fn add_friends(state: &AppState, a: i64, b: i64) -> ApiResult<Friendship> {
    if a == b {
        return Err(validation_error(&[("friendId", "An account cannot be its own friend")]));
    }
    if a == DEUS_PLAYER_ID || b == DEUS_PLAYER_ID {
        return Err(validation_error(&[("friendId", "Deus is a system account and cannot have friends")]));
    }
    if state.accounts.get_user(a).await?.is_none() || state.accounts.get_user(b).await?.is_none() {
        return Err(not_found("User not found"));
    }

    let mut row = match get_row(state, a, b).await? {
        Some(row) if row.status == FRIEND_ACCEPTED => return Err(conflict("They are already friends")),
        Some(row) => row,
        None => {
            let (low, high) = pair(a, b);
            match state.accounts.insert_friendship(low, high, a, FRIEND_PENDING).await {
                Ok(row) => row,
                Err(StoreError::Conflict) => return Err(conflict("They are already friends")),
                Err(other) => return Err(other.into()),
            }
        }
    };
    accept(state, &mut row).await?;
    Ok(row)
}

pub async fn neighbors_payload(state: &AppState, user_id: i64) -> ApiResult<Vec<Value>> {
    let ids = friend_ids(state, user_id).await?;
    let summaries = state.storage.players_summaries(&ids).await?;
    Ok(ids
        .iter()
        .filter_map(|id| {
            let summary = summaries.get(id)?;
            Some(json!({
                "pid": id.to_string(),
                "name": summary.name,
                "xp": summary.xp,
                "level": summary.level,
                "world_id": 0,
            }))
        })
        .collect())
}

pub async fn friends_flashvar(state: &AppState, user_id: i64) -> ApiResult<String> {
    let ids = friend_ids(state, user_id).await?;
    Ok(ids.iter().map(i64::to_string).collect::<Vec<_>>().join(","))
}

pub async fn require_friendship(state: &AppState, user_id: i64, other: i64) -> ApiResult<()> {
    if other == DEUS_PLAYER_ID {
        return Ok(());
    }
    if !are_friends(state, user_id, other).await? {
        return Err(not_found("Neighbour not found"));
    }
    Ok(())
}

pub async fn award_friend_gems(state: &AppState, player_id: i64, friend_id: i64) -> ApiResult<i64> {
    let _guard = state.storage.lock(player_id).await;
    let Some(mut loaded) = state.storage.load_player(player_id).await? else { return Ok(0) };

    let Some(root) = loaded.data.as_object_mut() else { return Ok(0) };
    let rewarded = ensure_array(ensure_object(root, "privateState"), "friendGemRewards");
    let already = rewarded.iter().any(|v| int_or(v, -1) == friend_id);
    if already || rewarded.len() >= FRIEND_REWARD_LIMIT {
        return Ok(0);
    }
    rewarded.push(Value::from(friend_id));
    let info = ensure_object(root, "playerInfo");
    crate::json::add_to(info, "cash", FRIEND_REWARD_GEMS);

    state.storage.save_player(player_id, &loaded.data).await?;
    tracing::info!("Player {player_id} received {FRIEND_REWARD_GEMS} gems for the friendship with {friend_id}");
    Ok(FRIEND_REWARD_GEMS)
}

pub async fn award_both(state: &AppState, a: i64, b: i64) -> ApiResult<(i64, i64)> {
    Ok((award_friend_gems(state, a, b).await?, award_friend_gems(state, b, a).await?))
}
