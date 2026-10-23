//! Accounts, passwords and login sessions, plus the request middleware that identifies the
use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::header::{self, HeaderValue};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::config::Settings;
use crate::db::User;
use crate::error::{forbidden, unauthorized, ApiError};
use crate::state::AppState;

pub mod passwords;
pub mod service;

pub const SESSION_COOKIE: &str = "dc_session";

#[derive(Clone, Default)]
pub struct Identity {
    pub user: Option<User>,
    pub token: Option<String>,
}

#[derive(Clone)]
pub struct ClientInfo {
    pub ip: String,
    pub user_agent: Option<String>,
}

pub struct Login(pub User);

pub struct Admin(pub User);

impl FromRequestParts<AppState> for Login {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &AppState) -> Result<Self, ApiError> {
        parts
            .extensions
            .get::<Identity>()
            .and_then(|identity| identity.user.clone())
            .map(Login)
            .ok_or_else(unauthorized)
    }
}

impl FromRequestParts<AppState> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &AppState) -> Result<Self, ApiError> {
        let user = parts
            .extensions
            .get::<Identity>()
            .and_then(|identity| identity.user.clone())
            .ok_or_else(unauthorized)?;
        if !user.is_admin {
            return Err(forbidden("Administrator access required"));
        }
        Ok(Admin(user))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Identity {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<Identity>().cloned().unwrap_or_default())
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ClientInfo {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<ClientInfo>()
            .cloned()
            .unwrap_or(ClientInfo { ip: "127.0.0.1".into(), user_agent: None }))
    }
}

fn header_text<'a>(headers: &'a HeaderMap, name: impl header::AsHeaderName) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

pub fn session_token(headers: &HeaderMap) -> Option<String> {
    for value in headers.get_all(header::COOKIE) {
        let Ok(text) = value.to_str() else { continue };
        for pair in text.split(';') {
            if let Some((name, token)) = pair.trim().split_once('=') {
                if name == SESSION_COOKIE && !token.is_empty() {
                    return Some(token.to_string());
                }
            }
        }
    }
    None
}

fn request_host(headers: &HeaderMap, settings: &Settings) -> String {
    let forwarded = settings
        .trust_proxy
        .then(|| header_text(headers, "x-forwarded-host"))
        .flatten()
        .and_then(|value| value.rsplit(',').next())
        .map(|v| v.trim().to_string());
    forwarded.or_else(|| header_text(headers, header::HOST).map(str::to_string)).unwrap_or_default()
}

fn client_ip(headers: &HeaderMap, peer: Option<SocketAddr>, settings: &Settings) -> String {
    if settings.trust_proxy {
        if let Some(last) = header_text(headers, "x-forwarded-for").and_then(|v| v.rsplit(',').next()) {
            let last = last.trim();
            if !last.is_empty() {
                return last.to_string();
            }
        }
    }
    peer.map(|p| p.ip().to_string()).unwrap_or_else(|| "127.0.0.1".to_string())
}

fn netloc(url: &str) -> &str {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    rest.split(['/', '?', '#']).next().unwrap_or("")
}

fn origin_allowed(origin: &str, host: &str, settings: &Settings) -> bool {
    let origin = origin.trim_end_matches('/');
    settings.allowed_origins.iter().any(|allowed| allowed == origin) || netloc(origin) == host
}

fn check_origin(method: &Method, headers: &HeaderMap, settings: &Settings) -> Result<(), ApiError> {
    if !matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE) {
        return Ok(());
    }
    let origin = match header_text(headers, header::ORIGIN) {
        Some(origin) => origin.to_string(),
        None => match header_text(headers, header::REFERER) {
            None => return Ok(()),
            Some(referer) => {
                let scheme = referer.split_once("://").map(|(scheme, _)| scheme).unwrap_or("");
                format!("{scheme}://{}", netloc(referer))
            }
        },
    };
    if origin == "null" || !origin_allowed(&origin, &request_host(headers, settings), settings) {
        return Err(forbidden("Cross-site request blocked"));
    }
    Ok(())
}

pub async fn request_context(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    if let Err(error) = check_origin(request.method(), request.headers(), &state.settings) {
        return finish(error.into_response(), request_path_is_api(&request));
    }

    let peer = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
    let info = ClientInfo {
        ip: client_ip(request.headers(), peer, &state.settings),
        user_agent: header_text(request.headers(), header::USER_AGENT).map(str::to_string),
    };
    let token = session_token(request.headers());
    let user = match &token {
        Some(token) => match service::resolve_session(&state, token).await {
            Ok(user) => user,
            Err(error) => return finish(error.into_response(), request_path_is_api(&request)),
        },
        None => None,
    };
    request.extensions_mut().insert(info);
    request.extensions_mut().insert(Identity { user, token });

    let is_api = request_path_is_api(&request);
    finish(next.run(request).await, is_api)
}

fn request_path_is_api(request: &Request) -> bool {
    request.uri().path().starts_with("/api/")
}

fn finish(mut response: Response, is_api: bool) -> Response {
    let headers = response.headers_mut();
    headers.entry("x-content-type-options").or_insert(HeaderValue::from_static("nosniff"));
    headers.entry("x-frame-options").or_insert(HeaderValue::from_static("DENY"));
    headers.entry("referrer-policy").or_insert(HeaderValue::from_static("same-origin"));
    if is_api {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

pub fn session_cookie(token: &str, settings: &Settings) -> HeaderValue {
    let secure = if settings.cookie_secure { "; Secure" } else { "" };
    let cookie = format!(
        "{SESSION_COOKIE}={token}; Max-Age={}; HttpOnly{secure}; SameSite=Lax; Path=/",
        settings.session_days * 86_400
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn clear_session_cookie(settings: &Settings) -> HeaderValue {
    let secure = if settings.cookie_secure { "; Secure" } else { "" };
    let cookie = format!(
        "{SESSION_COOKIE}=; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0{secure}; SameSite=Lax; Path=/"
    );
    HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static(""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netloc_is_taken_from_urls() {
        assert_eq!(netloc("https://evil.example"), "evil.example");
        assert_eq!(netloc("http://localhost:8080/path?x=1"), "localhost:8080");
        assert_eq!(netloc("localhost"), "localhost");
    }

    #[test]
    fn cookies_are_found_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_static("a=1; dc_session=tok; b=2"));
        assert_eq!(session_token(&headers).as_deref(), Some("tok"));
    }
}
