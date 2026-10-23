//! Test helpers: an in-memory application (no PostgreSQL or MongoDB) and a small HTTP client
#![allow(dead_code)]
use std::collections::HashMap;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use dragon_city::app::{build_router, in_memory_state};
use dragon_city::config::Settings;
use dragon_city::state::AppState;
use parking_lot::Mutex;
use serde_json::{json, Value};
use tower::ServiceExt;

pub const PASSWORD: &str = "correct horse battery";

pub struct TestApp {
    pub state: AppState,
    router: Router,
}

pub fn env_with(overrides: &[(&str, &str)]) -> HashMap<String, String> {
    let mut env = HashMap::new();
    for (key, value) in overrides {
        env.insert(key.to_string(), value.to_string());
    }
    env
}

impl TestApp {
    pub async fn new() -> TestApp {
        TestApp::with_env(&[]).await
    }

    pub async fn with_env(overrides: &[(&str, &str)]) -> TestApp {
        let settings = Settings::from_env(&env_with(overrides));
        let state = in_memory_state(settings).await.expect("the in-memory application starts");
        let router = build_router(state.clone());
        TestApp { state, router }
    }

    pub fn client(&self) -> Client {
        Client { router: self.router.clone(), cookie: Mutex::new(None) }
    }

    pub async fn register(&self, email: &str) -> (Client, i64) {
        self.state.limiter.reset();
        let client = self.client();
        let response = client.register(email).await;
        assert_eq!(response.status, StatusCode::CREATED, "{}", response.text());
        let id = response.json()["user"]["id"].as_i64().expect("the new user has an id");
        (client, id)
    }

    pub async fn player(&self) -> (Client, i64) {
        self.register("player@example.com").await
    }

    pub async fn admin(&self) -> Client {
        self.state.limiter.reset();
        dragon_city::auth::service::create_user(&self.state, "admin@example.com", PASSWORD, true)
            .await
            .expect("the admin is created");
        let client = self.client();
        let response = client.post_json("/api/auth/login", json!({"email": "admin@example.com", "password": PASSWORD})).await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
        client
    }

    pub async fn save(&self, player_id: i64) -> Option<Value> {
        self.state.storage.peek_player(player_id).await.expect("storage works")
    }

    pub async fn info(&self, player_id: i64) -> Value {
        self.save(player_id).await.expect("the player has a save")["playerInfo"].clone()
    }

    pub async fn gold(&self, player_id: i64) -> i64 {
        self.info(player_id).await["gold"].as_i64().unwrap()
    }

    pub async fn edit_save(&self, player_id: i64, mutate: impl FnOnce(&mut Value)) {
        let _guard = self.state.storage.lock(player_id).await;
        let mut loaded = self.state.storage.load_player(player_id).await.unwrap().expect("the player has a save");
        mutate(&mut loaded.data);
        self.state.storage.save_player(player_id, &loaded.data).await.unwrap();
    }

    pub async fn give(&self, player_id: i64, resources: Value) {
        self.edit_save(player_id, |data| {
            for (key, value) in resources.as_object().unwrap() {
                data["playerInfo"][key] = value.clone();
            }
        })
        .await;
    }

    pub async fn add_item(&self, player_id: i64, iso: i64, item_id: i64, attrs: Value) {
        self.edit_save(player_id, |data| {
            data["map"]["items"][iso.to_string()] = json!([item_id, 5, 5, 0, 0, 1, attrs]);
        })
        .await;
    }

    pub async fn befriend(&self, a: i64, b: i64) {
        dragon_city::friends::add_friends(&self.state, a, b).await.expect("they become friends");
    }
}

pub struct Response {
    pub status: StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", self.text()))
    }

    pub fn signed_json(&self) -> Value {
        let text = self.text();
        let (_, body) = text.split_once(';').expect("a signed response");
        serde_json::from_str(body).expect("signed JSON")
    }
}

pub struct Client {
    router: Router,
    cookie: Mutex<Option<String>>,
}

impl Client {
    pub async fn send(&self, method: Method, path: &str, extra_headers: &[(&str, &str)], content_type: Option<&str>, body: Vec<u8>) -> Response {
        let mut builder = Request::builder().method(method).uri(path).header(header::HOST, "localhost");
        if let Some(cookie) = self.cookie.lock().clone() {
            builder = builder.header(header::COOKIE, format!("dc_session={cookie}"));
        }
        if let Some(content_type) = content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        for (name, value) in extra_headers {
            builder = builder.header(*name, *value);
        }
        let request = builder.body(Body::from(body)).unwrap();
        let response = self.router.clone().oneshot(request).await.expect("the router answers");

        let (parts, body) = response.into_parts();
        for value in parts.headers.get_all(header::SET_COOKIE) {
            let text = value.to_str().unwrap_or("");
            if let Some(rest) = text.strip_prefix("dc_session=") {
                let token = rest.split(';').next().unwrap_or("");
                *self.cookie.lock() = if token.is_empty() { None } else { Some(token.to_string()) };
            }
        }
        let body = to_bytes(body, usize::MAX).await.unwrap().to_vec();
        Response { status: parts.status, headers: parts.headers, body }
    }

    pub async fn get(&self, path: &str) -> Response {
        self.send(Method::GET, path, &[], None, Vec::new()).await
    }

    pub async fn get_with(&self, path: &str, headers: &[(&str, &str)]) -> Response {
        self.send(Method::GET, path, headers, None, Vec::new()).await
    }

    pub async fn delete(&self, path: &str) -> Response {
        self.send(Method::DELETE, path, &[], None, Vec::new()).await
    }

    pub async fn post_empty(&self, path: &str) -> Response {
        self.send(Method::POST, path, &[], None, Vec::new()).await
    }

    pub async fn post_empty_with(&self, path: &str, headers: &[(&str, &str)]) -> Response {
        self.send(Method::POST, path, headers, None, Vec::new()).await
    }

    pub async fn json_request(&self, method: Method, path: &str, body: Value) -> Response {
        self.send(method, path, &[], Some("application/json"), serde_json::to_vec(&body).unwrap()).await
    }

    pub async fn post_json(&self, path: &str, body: Value) -> Response {
        self.json_request(Method::POST, path, body).await
    }

    pub async fn patch_json(&self, path: &str, body: Value) -> Response {
        self.json_request(Method::PATCH, path, body).await
    }

    pub async fn put_json(&self, path: &str, body: Value) -> Response {
        self.json_request(Method::PUT, path, body).await
    }

    pub async fn post_text(&self, path: &str, content_type: &str, body: &str) -> Response {
        self.send(Method::POST, path, &[], Some(content_type), body.as_bytes().to_vec()).await
    }

    pub async fn register(&self, email: &str) -> Response {
        self.post_json(
            "/api/auth/register",
            json!({"email": email, "password": PASSWORD, "confirmPassword": PASSWORD}),
        )
        .await
    }

    pub async fn post_packet_raw(&self, path: &str, data: &str, headers: &[(&str, &str)]) -> Response {
        let body = form_urlencoded::Serializer::new(String::new()).append_pair("data", data).finish();
        self.send(Method::POST, path, headers, Some("application/x-www-form-urlencoded"), body.into_bytes()).await
    }

    pub async fn packet(&self, commands: &[(&str, Value)]) -> Vec<Value> {
        let list: Vec<Value> = commands
            .iter()
            .enumerate()
            .map(|(i, (cmd, args))| json!({"cmd": cmd, "args": args, "number": i + 1}))
            .collect();
        let payload = format!("sig;{}", json!({"commands": list}));
        let response = self.post_packet_raw("/static/dragoncity/srv/packet.php", &payload, &[]).await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.text());
        response.signed_json()["response"].as_array().cloned().unwrap_or_default()
    }

    pub async fn player_info(&self, user: Option<i64>) -> Response {
        let suffix = user.map(|u| format!("?user={u}")).unwrap_or_default();
        self.get(&format!("/static/dragoncity/srv/get_player_info.json{suffix}")).await
    }
}

pub fn arc<T>(value: T) -> Arc<T> {
    Arc::new(value)
}
