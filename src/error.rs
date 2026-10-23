use std::collections::BTreeMap;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{json, Value};

#[derive(Debug)]
pub enum StoreError {
    Conflict,
    Other(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Conflict => write!(f, "unique constraint violated"),
            StoreError::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for StoreError {}

pub type StoreResult<T> = Result<T, StoreError>;

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &error {
            if db.is_unique_violation() {
                return StoreError::Conflict;
            }
        }
        StoreError::Other(error.to_string())
    }
}

impl From<mongodb::error::Error> for StoreError {
    fn from(error: mongodb::error::Error) -> Self {
        StoreError::Other(error.to_string())
    }
}

impl From<mongodb::bson::ser::Error> for StoreError {
    fn from(error: mongodb::bson::ser::Error) -> Self {
        StoreError::Other(error.to_string())
    }
}

impl From<mongodb::bson::de::Error> for StoreError {
    fn from(error: mongodb::bson::de::Error) -> Self {
        StoreError::Other(error.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: u16, code: &str, message: impl Into<String>) -> Self {
        ApiError {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            code: code.to_string(),
            message: message.into(),
            fields: BTreeMap::new(),
        }
    }

    pub fn with_fields(mut self, fields: BTreeMap<String, String>) -> Self {
        self.fields = fields;
        self
    }

    pub fn to_json(&self) -> Value {
        let mut body = json!({ "code": self.code, "message": self.message });
        if !self.fields.is_empty() {
            body["fields"] = json!(self.fields);
        }
        json!({ "error": body })
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, axum::Json(self.to_json())).into_response()
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        tracing::error!("Storage error: {error}");
        internal()
    }
}

pub fn internal() -> ApiError {
    ApiError::new(500, "internal_error", "Internal server error")
}

fn field_map(fields: &[(&str, &str)]) -> BTreeMap<String, String> {
    fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

pub fn validation_error(fields: &[(&str, &str)]) -> ApiError {
    ApiError::new(400, "validation_error", "Some fields are invalid").with_fields(field_map(fields))
}

pub fn unauthorized() -> ApiError {
    ApiError::new(401, "unauthorized", "Authentication required")
}

pub fn forbidden(message: &str) -> ApiError {
    ApiError::new(403, "forbidden", message)
}

pub fn not_found(message: &str) -> ApiError {
    ApiError::new(404, "not_found", message)
}

pub fn conflict(message: &str) -> ApiError {
    ApiError::new(409, "conflict", message)
}

pub fn conflict_with(message: &str, fields: &[(&str, &str)]) -> ApiError {
    ApiError::new(409, "conflict", message).with_fields(field_map(fields))
}

pub fn too_many_requests(message: &str) -> ApiError {
    ApiError::new(429, "too_many_requests", message)
}
