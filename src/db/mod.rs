//! Database layer: PostgreSQL (accounts, sessions, friendships) and MongoDB (game state).
use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::error::StoreResult;

pub mod memory;
pub mod mongo;
pub mod postgres;

pub const FIRST_PLAYER_ID: i64 = 200_000_000;

pub const FRIEND_PENDING: &str = "pending";
pub const FRIEND_ACCEPTED: &str = "accepted";
pub const FRIEND_DECLINED: &str = "declined";

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct LoginSession {
    pub token_hash: String,
    pub user_id: i64,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Friendship {
    pub id: i64,
    pub low_id: i64,
    pub high_id: i64,
    pub requester_id: i64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub responded_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct NewUser {
    pub id: Option<i64>,
    pub email: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub is_active: bool,
}

#[async_trait]
pub trait AccountStore: Send + Sync {
    async fn ping(&self) -> StoreResult<()>;

    async fn insert_user(&self, new: NewUser) -> StoreResult<User>;
    async fn get_user(&self, id: i64) -> StoreResult<Option<User>>;
    async fn get_user_by_email(&self, email: &str) -> StoreResult<Option<User>>;
    async fn update_user(&self, user: &User) -> StoreResult<()>;
    async fn delete_user(&self, id: i64) -> StoreResult<bool>;
    async fn list_users(&self, query: &str, offset: i64, limit: i64) -> StoreResult<(Vec<User>, i64)>;
    async fn users_by_ids(&self, ids: &[i64]) -> StoreResult<Vec<User>>;

    async fn insert_session(&self, session: &LoginSession) -> StoreResult<()>;
    async fn get_session(&self, token_hash: &str) -> StoreResult<Option<LoginSession>>;
    async fn touch_session(&self, token_hash: &str, last_seen: DateTime<Utc>, expires_at: DateTime<Utc>) -> StoreResult<()>;
    async fn delete_session(&self, token_hash: &str) -> StoreResult<()>;
    async fn delete_sessions_of(&self, user_id: i64) -> StoreResult<()>;
    async fn purge_expired_sessions(&self, now: DateTime<Utc>) -> StoreResult<u64>;

    async fn get_friendship(&self, id: i64) -> StoreResult<Option<Friendship>>;
    async fn get_friendship_pair(&self, low_id: i64, high_id: i64) -> StoreResult<Option<Friendship>>;
    async fn insert_friendship(&self, low_id: i64, high_id: i64, requester_id: i64, status: &str) -> StoreResult<Friendship>;
    async fn update_friendship(&self, friendship: &Friendship) -> StoreResult<()>;
    async fn delete_friendship(&self, id: i64) -> StoreResult<()>;
    async fn accepted_friend_ids(&self, user_id: i64) -> StoreResult<Vec<i64>>;
    async fn count_accepted(&self, user_id: i64) -> StoreResult<i64>;
    async fn count_pending_sent(&self, requester_id: i64) -> StoreResult<i64>;
    async fn pending_involving(&self, user_id: i64) -> StoreResult<Vec<Friendship>>;
    async fn count_incoming(&self, user_id: i64) -> StoreResult<i64>;

    async fn get_setting(&self, key: &str) -> StoreResult<Option<String>>;
    async fn set_setting(&self, key: &str, value: &str) -> StoreResult<()>;
}

#[derive(Clone, Debug)]
pub struct PlayerRecord {
    pub data: Value,
    pub kompu_offers: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct PendingAssist {
    pub helper: i64,
    pub clicks: Vec<Value>,
}

#[async_trait]
pub trait PlayerStore: Send + Sync {
    async fn ping(&self) -> StoreResult<()>;
    async fn ensure_indexes(&self) -> StoreResult<()>;

    async fn get_record(&self, id: i64) -> StoreResult<Option<PlayerRecord>>;
    async fn exists(&self, id: i64) -> StoreResult<bool>;
    async fn get_kompu_offers(&self, id: i64) -> StoreResult<Option<Value>>;
    async fn create(&self, id: i64, data: &Value, replace: bool) -> StoreResult<()>;
    async fn delete(&self, id: i64) -> StoreResult<()>;
    async fn save_data(&self, id: i64, data: &Value) -> StoreResult<()>;
    async fn set_kompu_offers(&self, id: i64, offers: &Value) -> StoreResult<()>;

    async fn pending_targets(&self) -> StoreResult<Vec<i64>>;
    async fn take_pending_assists(&self, target: i64) -> StoreResult<Vec<PendingAssist>>;
    async fn add_pending_assist(&self, target: i64, helper: i64, clicks: &[Value]) -> StoreResult<()>;

    async fn get_market(&self, id: i64) -> StoreResult<Option<Value>>;
    async fn save_market(&self, id: i64, market: &Value) -> StoreResult<()>;
    async fn get_minigame_ts(&self, id: i64) -> StoreResult<i64>;
    async fn set_minigame_ts(&self, id: i64, timestamp: i64) -> StoreResult<()>;

    async fn summaries(&self, ids: &[i64]) -> StoreResult<HashMap<i64, Value>>;

    async fn pvp_is_seeded(&self) -> StoreResult<bool>;
    async fn pvp_seed(&self, isle: Vec<Value>, players: Vec<(String, Value)>) -> StoreResult<()>;
    async fn pvp_page(&self, skip: i64, limit: i64) -> StoreResult<Vec<Value>>;
    async fn pvp_player(&self, id: &str) -> StoreResult<Option<Value>>;
    async fn pvp_entries(&self) -> StoreResult<Vec<Value>>;
    async fn pvp_count(&self) -> StoreResult<i64>;
    async fn pvp_insert(&self, id: &str, order: i64, entry: Value, data: Value) -> StoreResult<()>;
}
