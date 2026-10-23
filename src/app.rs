use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::Router;

use crate::config::Settings;
use crate::db::memory::{MemoryAccounts, MemoryPlayers};
use crate::error::ApiResult;
use crate::game::pvp::seed_pvp_data;
use crate::gamedata::GameData;
use crate::state::AppState;
use crate::storage::Storage;
use crate::{api, auth, routes, system_accounts};

const MAX_REQUEST_BYTES: usize = 1024 * 1024;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(api::auth::router())
        .merge(api::admin::router())
        .merge(api::friends::router())
        .merge(api::game::router())
        .merge(routes::srv::router())
        .merge(routes::packet::router())
        .merge(routes::market::router())
        .merge(routes::minigames::router())
        .merge(routes::pages::router(&state))
        .fallback(routes::pages::spa)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(middleware::from_fn_with_state(state.clone(), auth::request_context))
        .with_state(state)
}

pub async fn startup_tasks(state: &AppState) -> ApiResult<()> {
    state.storage.store().ensure_indexes().await?;
    seed_pvp_data(state).await?;
    system_accounts::ensure_deus(state).await?;
    let settings = &state.settings;
    if !settings.admin_email.is_empty() && !settings.admin_password.is_empty() {
        auth::service::ensure_admin(state, &settings.admin_email, &settings.admin_password).await?;
    }
    auth::service::purge_expired_sessions(state).await?;
    Ok(())
}

pub fn spawn_background_tasks(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            if let Err(error) = auth::service::purge_expired_sessions(&state).await {
                tracing::warn!("Could not purge the expired sessions: {}", error.message);
            }
            state.limiter.prune(Duration::from_secs(3600));
            state.logins.retain(|_, cached| cached.cached_at.elapsed() < crate::state::LOGIN_CACHE_TTL);
        }
    });
}

pub async fn in_memory_state(settings: Settings) -> Result<AppState, String> {
    let game = GameData::load(&settings.seed_dir)?;
    let storage = Storage::new(Arc::new(MemoryPlayers::new()), game).await.map_err(|e| e.to_string())?;
    let state = AppState::new(settings, Arc::new(MemoryAccounts::new()), storage);
    seed_pvp_data(&state).await.map_err(|e| e.message)?;
    Ok(state)
}
