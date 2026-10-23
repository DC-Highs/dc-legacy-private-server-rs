//! Accounts, sessions, CSRF, the web client, Deus, the admin API and the game versions.
mod common;

use axum::http::StatusCode;
use common::{TestApp, PASSWORD};
use dragon_city::config::DEUS_PLAYER_ID;
use dragon_city::system_accounts::{ensure_deus, DEUS_EMAIL};
use serde_json::json;

#[tokio::test]
async fn register_logs_in_with_an_httponly_cookie() {
    let app = TestApp::new().await;
    let client = app.client();
    let response = client.register("player@example.com").await;
    assert_eq!(response.status, StatusCode::CREATED);
    let cookie = response.headers.get("set-cookie").unwrap().to_str().unwrap().to_string();
    assert!(cookie.contains("dc_session=") && cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));

    let me = client.get("/api/auth/me").await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["user"]["email"], "player@example.com");
    assert_eq!(me.json()["user"]["isAdmin"], false);
    assert_eq!(me.json()["user"]["playerName"], "player");
}

#[tokio::test]
async fn the_password_confirmation_must_match() {
    let app = TestApp::new().await;
    let response = app
        .client()
        .post_json(
            "/api/auth/register",
            json!({"email": "a@example.com", "password": PASSWORD, "confirmPassword": format!("{PASSWORD}x")}),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert!(response.json()["error"]["fields"].get("confirmPassword").is_some());
}

#[tokio::test]
async fn weak_or_invalid_input_is_rejected() {
    let app = TestApp::new().await;
    let client = app.client();
    let short = client
        .post_json("/api/auth/register", json!({"email": "a@example.com", "password": "short", "confirmPassword": "short"}))
        .await;
    assert_eq!(short.status, StatusCode::BAD_REQUEST);
    assert!(short.json()["error"]["fields"].get("password").is_some());

    let bad_email = client
        .post_json("/api/auth/register", json!({"email": "nope", "password": PASSWORD, "confirmPassword": PASSWORD}))
        .await;
    assert_eq!(bad_email.status, StatusCode::BAD_REQUEST);
    assert!(bad_email.json()["error"]["fields"].get("email").is_some());

    let not_json = client.post_text("/api/auth/register", "text/plain", "email=a").await;
    assert_eq!(not_json.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn nobody_can_self_assign_admin_on_register() {
    let app = TestApp::new().await;
    let response = app
        .client()
        .post_json(
            "/api/auth/register",
            json!({"email": "a@example.com", "password": PASSWORD, "confirmPassword": PASSWORD, "isAdmin": true}),
        )
        .await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_duplicate_email_is_a_conflict_ignoring_case() {
    let app = TestApp::new().await;
    app.register("player@example.com").await;
    app.state.limiter.reset();
    assert_eq!(app.client().register("PLAYER@example.com").await.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn login_errors_do_not_reveal_whether_the_email_exists() {
    let app = TestApp::new().await;
    app.register("player@example.com").await;
    let fresh = app.client();
    let wrong = fresh.post_json("/api/auth/login", json!({"email": "player@example.com", "password": "wrong-password"})).await;
    let unknown = fresh.post_json("/api/auth/login", json!({"email": "nobody@example.com", "password": "wrong-password"})).await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.json(), unknown.json());
}

#[tokio::test]
async fn login_ignores_case_and_logout_ends_the_session() {
    let app = TestApp::new().await;
    app.register("player@example.com").await;
    let other = app.client();
    let login = other.post_json("/api/auth/login", json!({"email": "Player@Example.com", "password": PASSWORD})).await;
    assert_eq!(login.status, StatusCode::OK);
    assert_eq!(other.get("/api/auth/me").await.status, StatusCode::OK);
    assert_eq!(other.post_empty("/api/auth/logout").await.status, StatusCode::OK);
    assert_eq!(other.get("/api/auth/me").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn passwords_are_stored_hashed() {
    let app = TestApp::new().await;
    let (_client, id) = app.player().await;
    let user = app.state.accounts.get_user(id).await.unwrap().unwrap();
    assert!(user.password_hash.starts_with("$argon2id$") && !user.password_hash.contains(PASSWORD));
}

#[tokio::test]
async fn login_is_rate_limited() {
    let app = TestApp::new().await;
    app.register("player@example.com").await;
    let other = app.client();
    let mut statuses = Vec::new();
    for _ in 0..8 {
        let response = other.post_json("/api/auth/login", json!({"email": "player@example.com", "password": "bad-password"})).await;
        statuses.push(response.status);
    }
    assert!(statuses.contains(&StatusCode::TOO_MANY_REQUESTS));
}

#[tokio::test]
async fn cross_site_posts_are_blocked() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    let blocked = client.post_empty_with("/api/auth/logout", &[("origin", "https://evil.example")]).await;
    assert_eq!(blocked.status, StatusCode::FORBIDDEN);
    assert_eq!(client.get("/api/auth/me").await.status, StatusCode::OK);
}

#[tokio::test]
async fn same_origin_posts_and_null_origins() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    assert_eq!(client.post_empty_with("/api/auth/logout", &[("origin", "null")]).await.status, StatusCode::FORBIDDEN);
    assert_eq!(client.post_empty_with("/api/auth/logout", &[("origin", "http://localhost")]).await.status, StatusCode::OK);
}

#[tokio::test]
async fn game_packets_from_another_site_are_blocked() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    let response = client
        .post_packet_raw("/static/dragoncity/srv/packet.php", "{}", &[("origin", "https://evil.example")])
        .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn extra_allowed_origins_are_accepted() {
    let app = TestApp::with_env(&[("ALLOWED_ORIGINS", "https://game.example/")]).await;
    let (client, _) = app.player().await;
    assert_eq!(client.post_empty_with("/api/auth/logout", &[("origin", "https://game.example")]).await.status, StatusCode::OK);
}

fn dist() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("dist/_app")).unwrap();
    std::fs::write(dir.path().join("dist/index.html"), "<html>client</html>").unwrap();
    std::fs::write(dir.path().join("dist/_app/main.js"), "console.log(1)").unwrap();
    std::fs::write(dir.path().join("secret.txt"), "secret").unwrap();
    dir
}

#[tokio::test]
async fn health_checks_both_databases() {
    let app = TestApp::new().await;
    let response = app.client().get("/api/game/health").await;
    assert_eq!(response.json(), json!({"status": "ok"}));
}

#[tokio::test]
async fn the_web_client_is_served_with_a_history_fallback() {
    let dir = dist();
    let app = TestApp::with_env(&[("FRONTEND_DIST", dir.path().join("dist").to_str().unwrap())]).await;
    let client = app.client();
    assert!(client.get("/").await.text().contains("client"));
    assert!(client.get("/admin").await.text().contains("client"));
    assert_eq!(client.get("/_app/main.js").await.text(), "console.log(1)");
    assert_eq!(client.get("/missing.js").await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_web_client_cannot_read_outside_its_folder() {
    let dir = dist();
    let app = TestApp::with_env(&[("FRONTEND_DIST", dir.path().join("dist").to_str().unwrap())]).await;
    let client = app.client();
    assert!(!client.get("/%2e%2e/secret.txt").await.text().contains("secret"));
    assert!(!client.get("/../secret.txt").await.text().contains("secret"));
    assert!(!client.get("/..%5csecret.txt").await.text().contains("secret"));
}

#[tokio::test]
async fn unknown_api_paths_are_json_404s_not_the_web_client() {
    let dir = dist();
    let app = TestApp::with_env(&[("FRONTEND_DIST", dir.path().join("dist").to_str().unwrap())]).await;
    let client = app.client();
    let response = client.get("/api/nope").await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert_eq!(response.json()["error"]["code"], "not_found");
    assert!(matches!(client.get("/flash/x.swf").await.status, StatusCode::UNAUTHORIZED | StatusCode::NOT_FOUND));
}

#[tokio::test]
async fn a_missing_build_is_reported() {
    let app = TestApp::with_env(&[("FRONTEND_DIST", "does/not/exist")]).await;
    assert_eq!(app.client().get("/").await.status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn security_headers_are_set() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    let response = client.get("/api/auth/me").await;
    assert_eq!(response.headers["x-content-type-options"], "nosniff");
    assert_eq!(response.headers["x-frame-options"], "DENY");
    assert_eq!(response.headers["cache-control"], "no-store");
}

#[tokio::test]
async fn game_files_are_only_for_logged_in_players() {
    let flash = tempfile::tempdir().unwrap();
    std::fs::write(flash.path().join("DCLoader.swf"), b"FWS").unwrap();
    let app = TestApp::with_env(&[("FLASH_DIR", flash.path().to_str().unwrap())]).await;
    let anonymous = app.client();
    assert_eq!(anonymous.get("/flash/DCLoader.swf").await.status, StatusCode::UNAUTHORIZED);

    let (client, _) = app.player().await;
    let response = client.get("/flash/DCLoader.swf").await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body, b"FWS");
}

#[tokio::test]
async fn the_analytics_stubs_and_crossdomain_need_no_login() {
    let app = TestApp::new().await;
    let client = app.client();
    assert_eq!(client.get("/ga.js").await.headers["content-type"], "image/gif");
    assert!(client.get("/crossdomain.xml").await.text().contains("cross-domain-policy"));
}

#[tokio::test]
async fn registering_creates_a_save_and_packets_work() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    assert_eq!(app.save(id).await.unwrap()["playerInfo"]["pid"], id.to_string());
    let results = client.packet(&[("ping", json!([]))]).await;
    assert_eq!(results[0]["cmd"], "ping");
    assert_eq!(results[0]["result"], "ok");
}

#[tokio::test]
async fn deus_has_an_account_and_a_save() {
    let app = TestApp::new().await;
    ensure_deus(&app.state).await.unwrap();
    ensure_deus(&app.state).await.unwrap();
    let deus = app.state.accounts.get_user(DEUS_PLAYER_ID).await.unwrap().unwrap();
    assert!(deus.email == DEUS_EMAIL && !deus.is_active && !deus.is_admin);
    assert_eq!(app.save(DEUS_PLAYER_ID).await.unwrap()["playerInfo"]["name"], "Deus");
}

#[tokio::test]
async fn everybody_can_visit_deus_and_it_is_read_only() {
    let app = TestApp::new().await;
    ensure_deus(&app.state).await.unwrap();
    let (client, _) = app.player().await;
    let before = app.save(DEUS_PLAYER_ID).await.unwrap()["playerInfo"].clone();

    let response = client.player_info(Some(DEUS_PLAYER_ID)).await;
    assert_eq!(response.status, StatusCode::OK);
    let deus = response.signed_json();
    assert!(deus["playerInfo"]["name"] == "Deus" && deus["playerInfo"]["pid"] == DEUS_PLAYER_ID.to_string());
    assert_eq!(app.save(DEUS_PLAYER_ID).await.unwrap()["playerInfo"], before);
}

#[tokio::test]
async fn strangers_are_still_not_visitable() {
    let app = TestApp::new().await;
    ensure_deus(&app.state).await.unwrap();
    let (client, _) = app.player().await;
    let (_, other_id) = app.register("o@example.com").await;
    assert_eq!(client.player_info(Some(other_id)).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn nobody_can_log_in_as_deus_or_befriend_him() {
    let app = TestApp::new().await;
    ensure_deus(&app.state).await.unwrap();
    let (client, _) = app.player().await;
    let stranger = app.client();
    let login = stranger.post_json("/api/auth/login", json!({"email": DEUS_EMAIL, "password": "anything-at-all"})).await;
    assert!(matches!(login.status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN));
    let request = client.post_json("/api/friends/requests", json!({"playerId": DEUS_PLAYER_ID})).await;
    assert_eq!(request.status, StatusCode::NOT_FOUND);

    let info = client.player_info(None).await.text();
    let neighbors = info.split("\"neighbors\"").nth(1).unwrap();
    assert!(!neighbors[..neighbors.len().min(200)].contains(&DEUS_PLAYER_ID.to_string()));
}

#[tokio::test]
async fn admins_cannot_edit_or_delete_deus() {
    let app = TestApp::new().await;
    ensure_deus(&app.state).await.unwrap();
    let admin = app.admin().await;
    let url = format!("/api/admin/users/{DEUS_PLAYER_ID}");
    assert_eq!(admin.patch_json(&url, json!({"isActive": true})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(admin.delete(&url).await.status, StatusCode::FORBIDDEN);
    assert_eq!(admin.post_json(&format!("{url}/friends"), json!({"friendId": 200000000})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(admin.get(&url).await.status, StatusCode::OK);
}

#[tokio::test]
async fn the_admin_api_requires_an_admin() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    let admin = app.admin().await;
    assert_eq!(client.get("/api/admin/users").await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.client().get("/api/admin/users").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(admin.get("/api/admin/users").await.status, StatusCode::OK);
}

#[tokio::test]
async fn admins_create_list_search_and_inspect_users() {
    let app = TestApp::new().await;
    let admin = app.admin().await;
    let created = admin.post_json("/api/admin/users", json!({"email": "New@Example.com", "password": PASSWORD})).await;
    assert_eq!(created.status, StatusCode::CREATED);
    let user = created.json()["user"].clone();
    assert!(user["email"] == "new@example.com" && user["isAdmin"] == false && user["player"]["gold"].as_i64().unwrap() > 0);

    let listing = admin.get("/api/admin/users?query=new@").await.json();
    assert!(listing["total"] == 1 && listing["items"][0]["id"] == user["id"]);
    let by_id = admin.get(&format!("/api/admin/users?query={}", user["id"])).await.json();
    assert_eq!(by_id["total"], 1);
    assert_eq!(admin.get(&format!("/api/admin/users/{}", user["id"])).await.status, StatusCode::OK);
    assert_eq!(admin.get("/api/admin/users?query=%25").await.json()["total"], 0);
    assert_eq!(admin.get("/api/admin/users?page=x").await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_creation_rejects_duplicates_and_weak_passwords() {
    let app = TestApp::new().await;
    let admin = app.admin().await;
    let weak = admin.post_json("/api/admin/users", json!({"email": "a@example.com", "password": "short"})).await;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);
    let duplicate = admin.post_json("/api/admin/users", json!({"email": "admin@example.com", "password": PASSWORD})).await;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn disabling_or_changing_the_password_ends_the_sessions() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let admin = app.admin().await;
    assert_eq!(client.get("/api/auth/me").await.status, StatusCode::OK);
    assert_eq!(admin.patch_json(&format!("/api/admin/users/{id}"), json!({"isActive": false})).await.status, StatusCode::OK);
    assert_eq!(client.get("/api/auth/me").await.status, StatusCode::UNAUTHORIZED);

    let other = app.client();
    let login = |password: &str| json!({"email": "player@example.com", "password": password});
    assert_eq!(other.post_json("/api/auth/login", login(PASSWORD)).await.status, StatusCode::FORBIDDEN);

    let new_password = "another long password";
    admin.patch_json(&format!("/api/admin/users/{id}"), json!({"isActive": true, "password": new_password})).await;
    assert_eq!(other.post_json("/api/auth/login", login(PASSWORD)).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(other.post_json("/api/auth/login", login(new_password)).await.status, StatusCode::OK);
}

#[tokio::test]
async fn admins_cannot_lock_themselves_out() {
    let app = TestApp::new().await;
    let admin = app.admin().await;
    let me = admin.get("/api/auth/me").await.json()["user"]["id"].as_i64().unwrap();
    let url = format!("/api/admin/users/{me}");
    assert_eq!(admin.patch_json(&url, json!({"isAdmin": false})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(admin.patch_json(&url, json!({"isActive": false})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(admin.delete(&url).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn deleting_a_user_removes_the_account_and_the_save() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let admin = app.admin().await;
    assert_eq!(admin.delete(&format!("/api/admin/users/{id}")).await.status, StatusCode::OK);
    assert_eq!(admin.get(&format!("/api/admin/users/{id}")).await.status, StatusCode::NOT_FOUND);
    assert!(app.save(id).await.is_none());
    assert_eq!(client.get("/api/auth/me").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admins_edit_resources_and_reset_the_save() {
    let app = TestApp::new().await;
    let (_client, id) = app.player().await;
    let admin = app.admin().await;
    let url = format!("/api/admin/users/{id}/resources");
    assert_eq!(admin.patch_json(&url, json!({"gold": 123, "cash": 7})).await.status, StatusCode::OK);
    assert_eq!(app.gold(id).await, 123);
    assert_eq!(admin.patch_json(&url, json!({"gold": -1})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(admin.patch_json(&url, json!({"gold": 1_000_000_000_000i64})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(admin.patch_json(&url, json!({})).await.status, StatusCode::BAD_REQUEST);

    assert_eq!(admin.post_empty(&format!("/api/admin/users/{id}/reset-player")).await.status, StatusCode::OK);
    assert_ne!(app.gold(id).await, 123);
}

#[tokio::test]
async fn admins_cannot_set_unknown_fields() {
    let app = TestApp::new().await;
    let (_client, id) = app.player().await;
    let admin = app.admin().await;
    assert_eq!(admin.patch_json(&format!("/api/admin/users/{id}"), json!({"id": 1})).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admins_can_generate_pvp_opponents() {
    let app = TestApp::new().await;
    let admin = app.admin().await;
    let before = app.state.storage.store().pvp_count().await.unwrap();
    let response = admin.post_empty("/api/admin/pvp/generate").await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.json()["totalOpponents"].as_i64().unwrap(), before + 8);
}

async fn versions_app() -> (TestApp, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("versions")).unwrap();
    std::fs::write(dir.path().join("DCLoader.swf"), b"FWS").unwrap();
    for name in ["Base_sec_0.6.7c.swf", "Base_sec_0.10.0.swf", "Base_sec_0.7.9.swf", "Base_sec_0.6.7c - Flashpoint.swf"] {
        std::fs::write(dir.path().join("versions").join(name), b"FWS").unwrap();
    }
    std::fs::write(dir.path().join("versions/notes.txt"), "x").unwrap();
    let app = TestApp::with_env(&[("FLASH_DIR", dir.path().to_str().unwrap())]).await;
    (app, dir)
}

async fn swf_to_load(client: &common::Client) -> String {
    client.get("/api/game/bootstrap").await.json()["flashVars"]["swftoload"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn admins_list_the_versions_in_the_folder() {
    let (app, _dir) = versions_app().await;
    let admin = app.admin().await;
    let data = admin.get("/api/admin/game-version").await.json();
    let files: Vec<&str> = data["versions"].as_array().unwrap().iter().map(|v| v["file"].as_str().unwrap()).collect();
    assert_eq!(files, ["Base_sec_0.6.7c.swf", "Base_sec_0.7.9.swf", "Base_sec_0.10.0.swf"]);
    assert_eq!(data["versions"][0]["version"], "0.6.7c");
    assert!(data["selected"] == data["default"] && data["default"] == "Base_sec_0.6.7c.swf");
}

#[tokio::test]
async fn the_selected_version_is_what_the_game_loads() {
    let (app, _dir) = versions_app().await;
    let admin = app.admin().await;
    let response = admin.put_json("/api/admin/game-version", json!({"file": "Base_sec_0.7.9.swf"})).await;
    assert!(response.status == StatusCode::OK && response.json()["selected"] == "Base_sec_0.7.9.swf");
    assert_eq!(swf_to_load(&admin).await, "flash/versions/Base_sec_0.7.9.swf");

    let (other, _) = app.register("p@example.com").await;
    assert_eq!(swf_to_load(&other).await, "flash/versions/Base_sec_0.7.9.swf");
}

#[tokio::test]
async fn unknown_versions_are_refused() {
    let (app, _dir) = versions_app().await;
    let admin = app.admin().await;
    for name in ["Base_sec_9.9.9.swf", "DCLoader.swf", "../secret.swf", "Base_sec_0.6.7c - Flashpoint.swf"] {
        let response = admin.put_json("/api/admin/game-version", json!({"file": name})).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST, "{name}");
    }
    assert_eq!(admin.get("/api/admin/game-version").await.json()["selected"], "Base_sec_0.6.7c.swf");
}

#[tokio::test]
async fn a_removed_version_falls_back_to_the_default() {
    let (app, dir) = versions_app().await;
    let admin = app.admin().await;
    admin.put_json("/api/admin/game-version", json!({"file": "Base_sec_0.7.9.swf"})).await;
    std::fs::remove_file(dir.path().join("versions/Base_sec_0.7.9.swf")).unwrap();
    assert_eq!(swf_to_load(&admin).await, "flash/versions/Base_sec_0.6.7c.swf");
}

#[tokio::test]
async fn only_admins_choose_the_version() {
    let (app, _dir) = versions_app().await;
    let (client, _) = app.player().await;
    assert_eq!(client.get("/api/admin/game-version").await.status, StatusCode::FORBIDDEN);
    let response = client.put_json("/api/admin/game-version", json!({"file": "Base_sec_0.7.9.swf"})).await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
}
