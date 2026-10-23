//! Shared application state and the in-memory pieces that live next to it.
use std::ops::Deref;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::config::Settings;
use crate::db::{AccountStore, LoginSession, User};
use crate::storage::Storage;
use crate::util::RateLimiter;

const MAX_QUEUED_EVENTS: usize = 50;

#[derive(Default)]
pub struct SseHub {
    channels: DashMap<i64, broadcast::Sender<&'static str>>,
}

impl SseHub {
    pub fn subscribe(&self, player_id: i64) -> broadcast::Receiver<&'static str> {
        self.channels
            .entry(player_id)
            .or_insert_with(|| broadcast::channel(MAX_QUEUED_EVENTS).0)
            .subscribe()
    }

    pub fn notify(&self, player_id: i64, event: &'static str) {
        if let Some(sender) = self.channels.get(&player_id) {
            let _ = sender.send(event);
        }
    }

    pub fn release(&self, player_id: i64) {
        self.channels.remove_if(&player_id, |_, sender| sender.receiver_count() == 0);
    }
}

#[derive(Clone)]
pub struct CachedLogin {
    pub user: User,
    pub session: LoginSession,
    pub cached_at: Instant,
}

pub const LOGIN_CACHE_TTL: Duration = Duration::from_secs(10);

pub struct Inner {
    pub settings: Arc<Settings>,
    pub accounts: Arc<dyn AccountStore>,
    pub storage: Storage,
    pub limiter: RateLimiter,
    pub sse: SseHub,
    pub logins: DashMap<String, CachedLogin>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl AppState {
    pub fn new(settings: Settings, accounts: Arc<dyn AccountStore>, storage: Storage) -> AppState {
        AppState(Arc::new(Inner {
            settings: Arc::new(settings),
            accounts,
            storage,
            limiter: RateLimiter::new(),
            sse: SseHub::default(),
            logins: DashMap::new(),
        }))
    }
}

impl AppState {
    pub fn rate_limit(&self, key: &str, limits: &[crate::util::Limit]) -> crate::error::ApiResult<()> {
        if !self.settings.rate_limits_enabled || self.limiter.allow(key, limits) {
            Ok(())
        } else {
            Err(crate::error::too_many_requests("Too many requests, slow down"))
        }
    }
}

impl Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}
