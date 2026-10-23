use std::str::FromStr;
use std::sync::OnceLock;
use std::time::Instant;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use email_address::EmailAddress;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::passwords::{
    hash_password, hash_password_async, needs_rehash, verify_password_async, MAX_PASSWORD_LENGTH,
    MIN_PASSWORD_LENGTH,
};
use crate::db::{LoginSession, NewUser, User};
use crate::error::{conflict_with, validation_error, ApiError, ApiResult, StoreError};
use crate::state::{AppState, CachedLogin, LOGIN_CACHE_TTL};
use crate::storage::default_player_name;
use crate::util::token_urlsafe;

const SESSION_TOUCH_INTERVAL_HOURS: i64 = 1;

static DUMMY_HASH: OnceLock<String> = OnceLock::new();

pub fn normalize_email(email: &str) -> ApiResult<String> {
    let trimmed = email.trim();
    let valid = EmailAddress::from_str(trimmed)
        .ok()
        .filter(|parsed| parsed.domain().contains('.') && !trimmed.contains(char::is_whitespace));
    match valid {
        Some(parsed) => Ok(format!("{}@{}", parsed.local_part(), parsed.domain().to_lowercase()).to_lowercase()),
        None => Err(validation_error(&[("email", "Invalid email: the address is not valid")])),
    }
}

pub fn check_password(password: &str, email: &str) -> ApiResult<()> {
    let length = password.chars().count();
    if length < MIN_PASSWORD_LENGTH {
        return Err(validation_error(&[(
            "password",
            &format!("Password must have at least {MIN_PASSWORD_LENGTH} characters"),
        )]));
    }
    if length > MAX_PASSWORD_LENGTH {
        return Err(validation_error(&[(
            "password",
            &format!("Password must have at most {MAX_PASSWORD_LENGTH} characters"),
        )]));
    }
    if !email.is_empty() && password.to_lowercase() == email.to_lowercase() {
        return Err(validation_error(&[("password", "Password must not be your email")]));
    }
    Ok(())
}

pub async fn create_user(state: &AppState, email: &str, password: &str, is_admin: bool) -> ApiResult<User> {
    let email = normalize_email(email)?;
    check_password(password, &email)?;

    let password_hash = hash_password_async(password.to_string()).await;
    let new = NewUser { id: None, email: email.clone(), password_hash, is_admin, is_active: true };
    let user = match state.accounts.insert_user(new).await {
        Ok(user) => user,
        Err(StoreError::Conflict) => {
            return Err(conflict_with(
                "This email is already registered",
                &[("email", "This email is already registered")],
            ))
        }
        Err(other) => return Err(other.into()),
    };

    if let Err(error) = state.storage.create_player(user.id, &default_player_name(&email), false).await {
        let _ = state.accounts.delete_user(user.id).await;
        return Err(error.into());
    }
    tracing::info!("Created account {email} (id {}, admin={is_admin})", user.id);
    Ok(user)
}

pub async fn register_user(state: &AppState, email: &str, password: &str, confirm_password: &str) -> ApiResult<User> {
    if !state.settings.registration_enabled {
        return Err(ApiError::new(403, "registration_disabled", "Registration is disabled"));
    }
    if password != confirm_password {
        return Err(validation_error(&[("confirmPassword", "Passwords do not match")]));
    }
    create_user(state, email, password, false).await
}

fn invalid_credentials() -> ApiError {
    ApiError::new(401, "invalid_credentials", "Invalid email or password")
}

async fn burn_verification_time(password: &str) {
    let dummy = DUMMY_HASH.get_or_init(|| hash_password("not-a-real-password")).clone();
    let _ = verify_password_async(dummy, password.to_string()).await;
}

pub async fn authenticate(state: &AppState, email: &str, password: &str) -> ApiResult<User> {
    let Ok(email) = normalize_email(email) else {
        burn_verification_time(password).await;
        return Err(invalid_credentials());
    };

    let Some(mut user) = state.accounts.get_user_by_email(&email).await? else {
        burn_verification_time(password).await;
        return Err(invalid_credentials());
    };
    if !verify_password_async(user.password_hash.clone(), password.to_string()).await {
        return Err(invalid_credentials());
    }
    if !user.is_active {
        return Err(ApiError::new(403, "account_disabled", "This account is disabled"));
    }

    if needs_rehash(&user.password_hash) {
        user.password_hash = hash_password_async(password.to_string()).await;
    }
    user.last_login_at = Some(Utc::now());
    state.accounts.update_user(&user).await?;
    Ok(user)
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub async fn start_session(
    state: &AppState,
    user: &User,
    ip: Option<&str>,
    user_agent: Option<&str>,
) -> ApiResult<String> {
    let token = token_urlsafe(32);
    let now = Utc::now();
    let clip = |text: Option<&str>, max: usize| {
        text.map(|t| t.chars().take(max).collect::<String>()).filter(|t| !t.is_empty())
    };
    let session = LoginSession {
        token_hash: hash_token(&token),
        user_id: user.id,
        created_at: now,
        last_seen_at: now,
        expires_at: now + Duration::days(state.settings.session_days),
        ip: clip(ip, 64),
        user_agent: clip(user_agent, 255),
    };
    state.accounts.insert_session(&session).await?;
    Ok(token)
}

pub async fn resolve_session(state: &AppState, token: &str) -> ApiResult<Option<User>> {
    let token_hash = hash_token(token);
    let now = Utc::now();

    if let Some(cached) = state.logins.get(&token_hash) {
        if cached.cached_at.elapsed() < LOGIN_CACHE_TTL && cached.session.expires_at > now {
            return Ok(Some(cached.user.clone()));
        }
    }

    let Some(mut login) = state.accounts.get_session(&token_hash).await? else {
        state.logins.remove(&token_hash);
        return Ok(None);
    };
    if login.expires_at <= now {
        state.accounts.delete_session(&token_hash).await?;
        state.logins.remove(&token_hash);
        return Ok(None);
    }

    let user = match state.accounts.get_user(login.user_id).await? {
        Some(user) if user.is_active => user,
        _ => {
            state.logins.remove(&token_hash);
            return Ok(None);
        }
    };

    if now - login.last_seen_at > Duration::hours(SESSION_TOUCH_INTERVAL_HOURS) {
        let expires_at = now + Duration::days(state.settings.session_days);
        state.accounts.touch_session(&token_hash, now, expires_at).await?;
        login.last_seen_at = now;
        login.expires_at = expires_at;
    }

    state.logins.insert(token_hash, CachedLogin { user: user.clone(), session: login, cached_at: Instant::now() });
    Ok(Some(user))
}

pub async fn end_session(state: &AppState, token: &str) -> ApiResult<()> {
    let token_hash = hash_token(token);
    state.logins.remove(&token_hash);
    state.accounts.delete_session(&token_hash).await?;
    Ok(())
}

pub async fn end_all_sessions(state: &AppState, user_id: i64) -> ApiResult<()> {
    forget_user(state, user_id);
    state.accounts.delete_sessions_of(user_id).await?;
    Ok(())
}

pub fn forget_user(state: &AppState, user_id: i64) {
    state.logins.retain(|_, cached| cached.user.id != user_id);
}

pub async fn purge_expired_sessions(state: &AppState) -> ApiResult<u64> {
    Ok(state.accounts.purge_expired_sessions(Utc::now()).await?)
}

pub async fn ensure_player(state: &AppState, user: &User) -> ApiResult<()> {
    if !state.storage.player_exists(user.id).await? {
        state.storage.create_player(user.id, &default_player_name(&user.email), false).await?;
        tracing::warn!("Recreated the missing game save of account {}", user.id);
    }
    Ok(())
}

pub async fn ensure_admin(state: &AppState, email: &str, password: &str) -> ApiResult<Option<User>> {
    if email.is_empty() || password.is_empty() {
        return Ok(None);
    }
    let email = normalize_email(email)?;
    if let Some(existing) = state.accounts.get_user_by_email(&email).await? {
        return Ok(Some(existing));
    }
    Ok(Some(create_user(state, &email, password, true).await?))
}

pub fn iso(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Micros, false)
}

pub fn user_to_json(user: &User, player: Option<Value>) -> Value {
    let mut data = json!({
        "id": user.id,
        "email": user.email,
        "isAdmin": user.is_admin,
        "isActive": user.is_active,
        "createdAt": iso(user.created_at),
        "lastLoginAt": user.last_login_at.map(iso),
    });
    if let Some(player) = player {
        data["player"] = player;
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_validated_and_lowercased() {
        assert_eq!(normalize_email(" New@Example.COM ").unwrap(), "new@example.com");
        assert!(normalize_email("nope").is_err());
        assert!(normalize_email("a@b").is_err());
        assert!(normalize_email("a b@example.com").is_err());
    }

    #[test]
    fn passwords_have_length_limits_and_may_not_be_the_email() {
        assert!(check_password("short", "").is_err());
        assert!(check_password(&"x".repeat(129), "").is_err());
        assert!(check_password("a@example.com", "A@example.com").is_err());
        assert!(check_password("correct horse battery", "a@example.com").is_ok());
    }
}
