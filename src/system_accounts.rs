//! Accounts that belong to the game itself, not to a person.
use crate::auth::passwords::hash_password_async;
use crate::config::DEUS_PLAYER_ID;
use crate::db::{NewUser, User};
use crate::error::{ApiResult, StoreError};
use crate::state::AppState;
use crate::util::token_urlsafe;

pub const DEUS_NAME: &str = "Deus";
pub const DEUS_EMAIL: &str = "deus@system.dragoncity.invalid";

pub async fn ensure_deus(state: &AppState) -> ApiResult<User> {
    let user = match state.accounts.get_user(DEUS_PLAYER_ID).await? {
        Some(user) => user,
        None => {
            let new = NewUser {
                id: Some(DEUS_PLAYER_ID),
                email: DEUS_EMAIL.to_string(),
                password_hash: hash_password_async(token_urlsafe(48)).await,
                is_admin: false,
                is_active: false,
            };
            match state.accounts.insert_user(new).await {
                Ok(user) => {
                    tracing::info!("Created the system account of Deus (id {DEUS_PLAYER_ID})");
                    user
                }
                Err(StoreError::Conflict) => state
                    .accounts
                    .get_user(DEUS_PLAYER_ID)
                    .await?
                    .ok_or(StoreError::Conflict)?,
                Err(other) => return Err(other.into()),
            }
        }
    };

    if !state.storage.player_exists(DEUS_PLAYER_ID).await? {
        state.storage.create_player(DEUS_PLAYER_ID, DEUS_NAME, false).await?;
        tracing::info!("Created the save of Deus");
    }
    Ok(user)
}
