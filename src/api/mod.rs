use std::collections::BTreeMap;

use axum::body::Bytes;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{validation_error, ApiError, ApiResult};
use crate::state::AppState;

pub mod admin;
pub mod auth;
pub mod friends;
pub mod game;

pub struct Body<T>(pub T);

fn describe(message: &str) -> BTreeMap<String, String> {
    let field = message.split('`').nth(1).filter(|f| !f.is_empty()).unwrap_or("_");
    BTreeMap::from([(field.to_string(), message.to_string())])
}

impl<T: DeserializeOwned> FromRequest<AppState> for Body<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &AppState) -> Result<Self, ApiError> {
        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(|_| validation_error(&[("_", "A JSON object body is required")]))?;
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if !value.is_object() {
            return Err(validation_error(&[("_", "A JSON object body is required")]));
        }
        serde_json::from_value(value).map(Body).map_err(|error| {
            ApiError::new(400, "validation_error", "Some fields are invalid").with_fields(describe(&error.to_string()))
        })
    }
}

pub fn check_len(field: &str, value: &str, max: usize) -> ApiResult<()> {
    if value.chars().count() > max {
        return Err(validation_error(&[(field, &format!("String should have at most {max} characters"))]));
    }
    Ok(())
}

pub fn check_range(field: &str, value: i64, min: i64, max: i64) -> ApiResult<()> {
    if value < min || value > max {
        return Err(validation_error(&[(field, &format!("Input should be between {min} and {max}"))]));
    }
    Ok(())
}

pub fn json_status(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}
