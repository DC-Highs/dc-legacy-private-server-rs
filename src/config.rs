use std::collections::HashMap;
use std::path::PathBuf;

use sqlx::postgres::PgConnectOptions;

pub const SECRET_KEY: &str = "RGhXbiy4xEeDnSNX1oBG";

pub const PVP_PAGE_SIZE: i64 = 8;

pub const DEUS_PLAYER_ID: i64 = 100_000_030;
pub const SSE_KEEPALIVE_SECONDS: u64 = 20;

pub const PIXEL_GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";

pub const LEGACY_GAME_HOSTS: [&str; 8] = [
    "static.socialpointgames.com",
    "sp-translations.socialpointgames.com",
    "api.omniata.com",
    "graph.facebook.com",
    "ajax.googleapis.com",
    "www.google-analytics.com",
    "ssl.google-analytics.com",
    "google-analytics.com",
];

#[derive(Clone, Debug)]
pub enum DatabaseConfig {
    Url(String),
    Parts { host: String, port: u16, user: String, password: String, database: String },
}

impl DatabaseConfig {
    pub fn connect_options(&self) -> Result<PgConnectOptions, String> {
        match self {
            DatabaseConfig::Url(url) => {
                let normalized = match url.split_once("://") {
                    Some((scheme, rest)) if scheme.starts_with("postgres") => format!("postgres://{rest}"),
                    _ => url.clone(),
                };
                normalized.parse::<PgConnectOptions>().map_err(|e| format!("invalid DATABASE_URL: {e}"))
            }
            DatabaseConfig::Parts { host, port, user, password, database } => Ok(PgConnectOptions::new()
                .host(host)
                .port(*port)
                .username(user)
                .password(password)
                .database(database)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub database: DatabaseConfig,
    pub mongo_url: String,
    pub mongo_db: String,
    pub mongo_user: String,
    pub mongo_password: String,
    pub cookie_secure: bool,
    pub session_days: i64,
    pub max_friends: i64,
    pub max_pending_requests: i64,
    pub registration_enabled: bool,
    pub rate_limits_enabled: bool,
    pub allowed_origins: Vec<String>,
    pub trust_proxy: bool,
    pub admin_email: String,
    pub admin_password: String,
    pub swf_version: String,
    pub log_level: String,
    pub bind_addr: String,
    pub seed_dir: PathBuf,
    pub flash_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub domain_map_dir: PathBuf,
    pub frontend_dist: PathBuf,
}

fn flag(value: Option<&String>, default: bool) -> bool {
    match value.map(|v| v.trim().to_lowercase()) {
        None => default,
        Some(v) if v.is_empty() => default,
        Some(v) => matches!(v.as_str(), "1" | "true" | "yes" | "on"),
    }
}

fn number(env: &HashMap<String, String>, name: &str, default: i64) -> i64 {
    env.get(name).and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

fn path(env: &HashMap<String, String>, name: &str, default: &str) -> PathBuf {
    match env.get(name) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(default),
    }
}

fn text(env: &HashMap<String, String>, name: &str, default: &str) -> String {
    env.get(name).cloned().unwrap_or_else(|| default.to_string())
}

impl Settings {
    pub fn from_process_env() -> Settings {
        Settings::from_env(&std::env::vars().collect())
    }

    pub fn from_env(env: &HashMap<String, String>) -> Settings {
        let database = match env.get("DATABASE_URL") {
            Some(url) if !url.is_empty() => DatabaseConfig::Url(url.clone()),
            _ => DatabaseConfig::Parts {
                host: text(env, "POSTGRES_HOST", "localhost"),
                port: number(env, "POSTGRES_PORT", 5432) as u16,
                user: text(env, "POSTGRES_USER", "dragon"),
                password: text(env, "POSTGRES_PASSWORD", "dragon"),
                database: text(env, "POSTGRES_DB", "dragoncity"),
            },
        };
        let allowed_origins = text(env, "ALLOWED_ORIGINS", "")
            .split(',')
            .map(|o| o.trim().trim_end_matches('/').to_string())
            .filter(|o| !o.is_empty())
            .collect();
        let log_level = env
            .get("LOG_LEVEL")
            .or_else(|| env.get("DRAGON_CITY_LOG_LEVEL"))
            .cloned()
            .unwrap_or_else(|| "INFO".to_string());

        Settings {
            database,
            mongo_url: text(env, "MONGO_URL", "mongodb://localhost:27017"),
            mongo_db: text(env, "MONGO_DB", "dragoncity"),
            mongo_user: text(env, "MONGO_USER", ""),
            mongo_password: text(env, "MONGO_PASSWORD", ""),
            cookie_secure: flag(env.get("COOKIE_SECURE"), false),
            session_days: number(env, "SESSION_DAYS", 14),
            max_friends: number(env, "MAX_FRIENDS", 20).max(1),
            max_pending_requests: number(env, "MAX_PENDING_REQUESTS", 5).max(1),
            registration_enabled: flag(env.get("REGISTRATION_ENABLED"), true),
            rate_limits_enabled: !flag(env.get("DISABLE_RATE_LIMITS"), false),
            allowed_origins,
            trust_proxy: flag(env.get("TRUST_PROXY"), false),
            admin_email: text(env, "ADMIN_EMAIL", ""),
            admin_password: text(env, "ADMIN_PASSWORD", ""),
            swf_version: text(env, "GAME_SWF", "Base_sec_0.6.7c.swf"),
            log_level,
            bind_addr: text(env, "BIND_ADDR", "0.0.0.0:8080"),
            seed_dir: path(env, "SEED_DIR", "web/srv"),
            flash_dir: path(env, "FLASH_DIR", "flash"),
            assets_dir: path(env, "ASSETS_DIR", "assets"),
            domain_map_dir: path(env, "DOMAIN_MAP_DIR", "domains"),
            frontend_dist: path(env, "FRONTEND_DIST", "frontend/dist"),
        }
    }
}
