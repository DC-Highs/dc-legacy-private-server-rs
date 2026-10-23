use std::path::{Component, Path as FsPath, PathBuf};

use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use percent_encoding::percent_decode_str;
use serde_json::json;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::auth::Identity;
use crate::config::PIXEL_GIF;
use crate::error::{not_found, unauthorized, ApiError};
use crate::state::AppState;

const DEFAULT_CROSSDOMAIN: &str =
    r#"<?xml version="1.0"?><cross-domain-policy><allow-access-from domain="*"/></cross-domain-policy>"#;
const RESERVED_PREFIXES: [&str; 6] = ["api/", "static/", "flash/", "assets/", "deploy/", "stream"];

async fn require_login(request: Request, next: Next) -> Response {
    let logged_in = request.extensions().get::<Identity>().map(|i| i.user.is_some()).unwrap_or(false);
    if !logged_in {
        return unauthorized().into_response();
    }
    next.run(request).await
}

pub fn router(state: &AppState) -> Router<AppState> {
    let settings = &state.settings;
    let revalidate = HeaderValue::from_static("no-cache");
    let cacheable = HeaderValue::from_static("private, max-age=3600");

    let protected = Router::new()
        .nest_service(
            "/flash",
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, revalidate))
                .service(ServeDir::new(&settings.flash_dir)),
        )
        .nest_service(
            "/assets",
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, cacheable))
                .service(ServeDir::new(&settings.assets_dir)),
        )
        .route("/deploy/dc/canvas/prod/{filename}", get(serve_translation))
        .layer(middleware::from_fn(require_login));

    Router::new()
        .merge(protected)
        .route("/sp-translations.socialpointgames.com/crossdomain.xml", get(crossdomain))
        .route("/crossdomain.xml", get(crossdomain))
        .route("/__utm.gif", get(ga_stub))
        .route("/ga.js", get(ga_stub))
}

async fn crossdomain(State(state): State<AppState>) -> Response {
    let file = state.settings.domain_map_dir.join("api.omniata.com").join("crossdomain.xml");
    match tokio::fs::read(&file).await {
        Ok(content) => ([(header::CONTENT_TYPE, "application/xml")], content).into_response(),
        Err(_) => ([(header::CONTENT_TYPE, "application/xml")], DEFAULT_CROSSDOMAIN).into_response(),
    }
}

async fn ga_stub() -> Response {
    ([(header::CONTENT_TYPE, "image/gif")], PIXEL_GIF).into_response()
}

async fn serve_translation(State(state): State<AppState>, Path(filename): Path<String>, request: Request) -> Response {
    let directory = state
        .settings
        .domain_map_dir
        .join("sp-translations.socialpointgames.com")
        .join("deploy")
        .join("dc")
        .join("canvas")
        .join("prod");
    let plain_name = !filename.is_empty() && !filename.contains(['/', '\\']) && filename != "." && filename != "..";
    let file = directory.join(&filename);
    if plain_name && file.is_file() {
        return serve_file(file, request).await;
    }
    (StatusCode::NOT_FOUND, Json(json!({ "error": "Translation file not found" }))).into_response()
}

fn safe_join(root: &FsPath, relative: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    for component in FsPath::new(relative).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if relative.contains(['\\', '\0']) {
        return None;
    }
    Some(path)
}

pub async fn spa(State(state): State<AppState>, uri: Uri, request: Request) -> Response {
    let decoded = percent_decode_str(uri.path()).decode_utf8_lossy().to_string();
    let path = decoded.trim_start_matches('/');

    if RESERVED_PREFIXES.iter().any(|prefix| path.starts_with(prefix)) {
        return not_found("Not found").into_response();
    }

    let dist = &state.settings.frontend_dist;
    let index = dist.join("index.html");
    if !index.is_file() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "The web client is not built (frontend/dist is missing)" })),
        )
            .into_response();
    }

    if !path.is_empty() {
        if let Some(candidate) = safe_join(dist, path).filter(|c| c.is_file()) {
            return serve_file(candidate, request).await;
        }
        if path.rsplit('/').next().unwrap_or("").contains('.') {
            return ApiError::new(404, "not_found", "Not found").into_response();
        }
    }
    serve_file(index, request).await
}

async fn serve_file(path: PathBuf, request: Request) -> Response {
    match ServeFile::new(path).oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(never) => match never {},
    }
}
