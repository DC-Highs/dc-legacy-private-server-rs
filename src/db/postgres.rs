use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use super::{AccountStore, Friendship, LoginSession, NewUser, User, FRIEND_ACCEPTED, FRIEND_PENDING};
use crate::config::DatabaseConfig;
use crate::error::{StoreError, StoreResult};

pub struct PgAccounts {
    pool: PgPool,
}

pub async fn connect(config: &DatabaseConfig, wait_seconds: u64) -> Result<PgPool, StoreError> {
    let options = config.connect_options().map_err(StoreError::Other)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait_seconds);
    loop {
        let attempt = PgPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect_with(options.clone())
            .await;
        match attempt {
            Ok(pool) => return Ok(pool),
            Err(error) if std::time::Instant::now() < deadline => {
                tracing::info!("Waiting for PostgreSQL... ({error})");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

pub async fn migrate(pool: &PgPool) -> Result<(), StoreError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| StoreError::Other(format!("migration failed: {e}")))
}

impl PgAccounts {
    pub fn new(pool: PgPool) -> PgAccounts {
        PgAccounts { pool }
    }
}

fn like_pattern(query: &str) -> String {
    let escaped = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

#[async_trait]
impl AccountStore for PgAccounts {
    async fn ping(&self) -> StoreResult<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn insert_user(&self, new: NewUser) -> StoreResult<User> {
        let user = match new.id {
            Some(id) => {
                sqlx::query_as::<_, User>(
                    "INSERT INTO users (id, email, password_hash, is_admin, is_active, created_at) \
                     VALUES ($1, $2, $3, $4, $5, now()) RETURNING *",
                )
                .bind(id)
                .bind(&new.email)
                .bind(&new.password_hash)
                .bind(new.is_admin)
                .bind(new.is_active)
                .fetch_one(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, User>(
                    "INSERT INTO users (email, password_hash, is_admin, is_active, created_at) \
                     VALUES ($1, $2, $3, $4, now()) RETURNING *",
                )
                .bind(&new.email)
                .bind(&new.password_hash)
                .bind(new.is_admin)
                .bind(new.is_active)
                .fetch_one(&self.pool)
                .await?
            }
        };
        Ok(user)
    }

    async fn get_user(&self, id: i64) -> StoreResult<Option<User>> {
        Ok(sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1").bind(id).fetch_optional(&self.pool).await?)
    }

    async fn get_user_by_email(&self, email: &str) -> StoreResult<Option<User>> {
        Ok(sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn update_user(&self, user: &User) -> StoreResult<()> {
        sqlx::query(
            "UPDATE users SET email = $2, password_hash = $3, is_admin = $4, is_active = $5, last_login_at = $6 \
             WHERE id = $1",
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(user.is_admin)
        .bind(user.is_active)
        .bind(user.last_login_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_user(&self, id: i64) -> StoreResult<bool> {
        let result = sqlx::query("DELETE FROM users WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_users(&self, query: &str, offset: i64, limit: i64) -> StoreResult<(Vec<User>, i64)> {
        if query.is_empty() {
            let total: i64 = sqlx::query_scalar("SELECT count(*) FROM users").fetch_one(&self.pool).await?;
            let users = sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY id DESC OFFSET $1 LIMIT $2")
                .bind(offset)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;
            return Ok((users, total));
        }

        let pattern = like_pattern(query);
        let numeric_id: Option<i64> = query.chars().all(|c| c.is_ascii_digit()).then(|| query.parse().ok()).flatten();
        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email ILIKE $1 ESCAPE '\\' OR id = $2")
            .bind(&pattern)
            .bind(numeric_id)
            .fetch_one(&self.pool)
            .await?;
        let users = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE email ILIKE $1 ESCAPE '\\' OR id = $2 ORDER BY id DESC OFFSET $3 LIMIT $4",
        )
        .bind(&pattern)
        .bind(numeric_id)
        .bind(offset)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok((users, total))
    }

    async fn users_by_ids(&self, ids: &[i64]) -> StoreResult<Vec<User>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(&self.pool)
            .await?)
    }

    async fn insert_session(&self, session: &LoginSession) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, created_at, last_seen_at, expires_at, ip, user_agent) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&session.token_hash)
        .bind(session.user_id)
        .bind(session.created_at)
        .bind(session.last_seen_at)
        .bind(session.expires_at)
        .bind(&session.ip)
        .bind(&session.user_agent)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session(&self, token_hash: &str) -> StoreResult<Option<LoginSession>> {
        Ok(sqlx::query_as::<_, LoginSession>("SELECT * FROM sessions WHERE token_hash = $1")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn touch_session(&self, token_hash: &str, last_seen: DateTime<Utc>, expires_at: DateTime<Utc>) -> StoreResult<()> {
        sqlx::query("UPDATE sessions SET last_seen_at = $2, expires_at = $3 WHERE token_hash = $1")
            .bind(token_hash)
            .bind(last_seen)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_session(&self, token_hash: &str) -> StoreResult<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(token_hash).execute(&self.pool).await?;
        Ok(())
    }

    async fn delete_sessions_of(&self, user_id: i64) -> StoreResult<()> {
        sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(user_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn purge_expired_sessions(&self, now: DateTime<Utc>) -> StoreResult<u64> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= $1").bind(now).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn get_friendship(&self, id: i64) -> StoreResult<Option<Friendship>> {
        Ok(sqlx::query_as::<_, Friendship>("SELECT * FROM friendships WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn get_friendship_pair(&self, low_id: i64, high_id: i64) -> StoreResult<Option<Friendship>> {
        Ok(sqlx::query_as::<_, Friendship>("SELECT * FROM friendships WHERE low_id = $1 AND high_id = $2")
            .bind(low_id)
            .bind(high_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn insert_friendship(&self, low_id: i64, high_id: i64, requester_id: i64, status: &str) -> StoreResult<Friendship> {
        Ok(sqlx::query_as::<_, Friendship>(
            "INSERT INTO friendships (low_id, high_id, requester_id, status, created_at) \
             VALUES ($1, $2, $3, $4, now()) RETURNING *",
        )
        .bind(low_id)
        .bind(high_id)
        .bind(requester_id)
        .bind(status)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn update_friendship(&self, friendship: &Friendship) -> StoreResult<()> {
        sqlx::query(
            "UPDATE friendships SET requester_id = $2, status = $3, created_at = $4, responded_at = $5 WHERE id = $1",
        )
        .bind(friendship.id)
        .bind(friendship.requester_id)
        .bind(&friendship.status)
        .bind(friendship.created_at)
        .bind(friendship.responded_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_friendship(&self, id: i64) -> StoreResult<()> {
        sqlx::query("DELETE FROM friendships WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    async fn accepted_friend_ids(&self, user_id: i64) -> StoreResult<Vec<i64>> {
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT low_id, high_id FROM friendships \
             WHERE status = $2 AND (low_id = $1 OR high_id = $1) ORDER BY id",
        )
        .bind(user_id)
        .bind(FRIEND_ACCEPTED)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(low, high)| if low == user_id { high } else { low }).collect())
    }

    async fn count_accepted(&self, user_id: i64) -> StoreResult<i64> {
        Ok(sqlx::query_scalar("SELECT count(*) FROM friendships WHERE status = $2 AND (low_id = $1 OR high_id = $1)")
            .bind(user_id)
            .bind(FRIEND_ACCEPTED)
            .fetch_one(&self.pool)
            .await?)
    }

    async fn count_pending_sent(&self, requester_id: i64) -> StoreResult<i64> {
        Ok(sqlx::query_scalar("SELECT count(*) FROM friendships WHERE status = $2 AND requester_id = $1")
            .bind(requester_id)
            .bind(FRIEND_PENDING)
            .fetch_one(&self.pool)
            .await?)
    }

    async fn pending_involving(&self, user_id: i64) -> StoreResult<Vec<Friendship>> {
        Ok(sqlx::query_as::<_, Friendship>(
            "SELECT * FROM friendships WHERE status = $2 AND (low_id = $1 OR high_id = $1) ORDER BY id DESC",
        )
        .bind(user_id)
        .bind(FRIEND_PENDING)
        .fetch_all(&self.pool)
        .await?)
    }

    async fn count_incoming(&self, user_id: i64) -> StoreResult<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(*) FROM friendships \
             WHERE status = $2 AND (low_id = $1 OR high_id = $1) AND requester_id <> $1",
        )
        .bind(user_id)
        .bind(FRIEND_PENDING)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn get_setting(&self, key: &str) -> StoreResult<Option<String>> {
        Ok(sqlx::query_scalar("SELECT value FROM app_settings WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn set_setting(&self, key: &str, value: &str) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO app_settings (key, value, updated_at) VALUES ($1, $2, now()) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
