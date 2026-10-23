mod common;

use axum::http::StatusCode;
use common::{Client, TestApp};
use dragon_city::db::FRIEND_DECLINED;
use dragon_city::friends::FRIEND_REWARD_LIMIT;
use serde_json::{json, Value};

async fn gems(app: &TestApp, player_id: i64) -> i64 {
    app.info(player_id).await["cash"].as_i64().unwrap()
}

async fn overview(client: &Client) -> Value {
    client.get("/api/friends").await.json()
}

async fn ask(client: &Client, player_id: i64) -> common::Response {
    client.post_json("/api/friends/requests", json!({"playerId": player_id})).await
}

async fn first_incoming(client: &Client) -> i64 {
    overview(client).await["incoming"][0]["requestId"].as_i64().unwrap()
}

async fn accept_first(client: &Client) -> common::Response {
    let id = first_incoming(client).await;
    client.post_empty(&format!("/api/friends/requests/{id}/accept")).await
}

fn friend_ids(overview: &Value) -> Vec<i64> {
    overview["friends"].as_array().unwrap().iter().map(|f| f["id"].as_i64().unwrap()).collect()
}

async fn pair(app: &TestApp) -> (Client, i64, Client, i64) {
    let (alice, alice_id) = app.register("alice@example.com").await;
    let (bob, bob_id) = app.register("bob@example.com").await;
    (alice, alice_id, bob, bob_id)
}

#[tokio::test]
async fn a_new_player_has_no_neighbours() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    assert_eq!(client.player_info(None).await.signed_json()["neighbors"], json!([]));
    assert_eq!(client.get("/api/game/bootstrap").await.json()["flashVars"]["fb_sig_friends"], "");
}

#[tokio::test]
async fn friends_appear_as_neighbours_in_the_game() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (friend_client, friend_id) = app.register("friend@example.com").await;
    app.befriend(id, friend_id).await;

    let info = client.player_info(None).await.signed_json();
    assert_eq!(
        info["neighbors"],
        json!([{"pid": friend_id.to_string(), "name": "friend", "xp": 0, "level": 1, "world_id": 0}])
    );
    assert_eq!(client.get("/api/game/bootstrap").await.json()["flashVars"]["fb_sig_friends"], friend_id.to_string());
    assert_eq!(friend_client.player_info(None).await.signed_json()["neighbors"][0]["pid"], id.to_string());
    assert_eq!(app.save(id).await.unwrap()["neighbors"], json!([]));
}

#[tokio::test]
async fn neighbours_show_their_current_experience() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (friend_client, friend_id) = app.register("friend@example.com").await;
    app.befriend(id, friend_id).await;
    friend_client.packet(&[("buy", json!(["700", 16, 3, 3, 0, 0, 1]))]).await;
    assert_eq!(client.player_info(None).await.signed_json()["neighbors"][0]["xp"], 100);
}

#[tokio::test]
async fn only_friends_can_be_visited() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (_friend, friend_id) = app.register("friend@example.com").await;
    assert_eq!(client.player_info(Some(friend_id)).await.status, StatusCode::NOT_FOUND);

    app.befriend(id, friend_id).await;
    let visit = client.player_info(Some(friend_id)).await;
    assert_eq!(visit.status, StatusCode::OK);
    assert_eq!(visit.signed_json()["playerInfo"]["pid"], friend_id.to_string());
}

#[tokio::test]
async fn only_friends_can_be_assisted() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (friend_client, friend_id) = app.register("friend@example.com").await;
    client.packet(&[("assist_neighbor", json!([friend_id, "[1,2]"]))]).await;
    friend_client.packet(&[("ping", json!([]))]).await;
    assert!(app.save(friend_id).await.unwrap()["privateState"].get("assistRewardsLeft").is_none());

    app.befriend(id, friend_id).await;
    client.packet(&[("assist_neighbor", json!([friend_id, "[1,2]"]))]).await;
    friend_client.packet(&[("ping", json!([]))]).await;
    assert_eq!(app.save(friend_id).await.unwrap()["privateState"]["assistRewardsLeft"], 2);
}

#[tokio::test]
async fn admins_manage_friendships() {
    let app = TestApp::new().await;
    let admin = app.admin().await;
    let (_client, id) = app.player().await;
    let (_friend, friend_id) = app.register("friend@example.com").await;
    let url = format!("/api/admin/users/{id}/friends");

    assert_eq!(admin.get(&url).await.json(), json!({"items": []}));
    let added = admin.post_json(&url, json!({"friendId": friend_id})).await;
    assert!(added.status == StatusCode::CREATED && added.json()["items"][0]["id"] == friend_id);
    assert_eq!(admin.get(&format!("/api/admin/users/{friend_id}/friends")).await.json()["items"][0]["id"], id);

    assert_eq!(admin.post_json(&url, json!({"friendId": friend_id})).await.status, StatusCode::CONFLICT);
    let reverse = admin.post_json(&format!("/api/admin/users/{friend_id}/friends"), json!({"friendId": id})).await;
    assert_eq!(reverse.status, StatusCode::CONFLICT);
    assert_eq!(admin.post_json(&url, json!({"friendId": id})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(admin.post_json(&url, json!({"friendId": 999_999_999})).await.status, StatusCode::NOT_FOUND);
    assert_eq!(admin.post_json(&url, json!({"friendId": "x"})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(admin.post_json(&url, json!({"friendId": friend_id, "extra": 1})).await.status, StatusCode::BAD_REQUEST);

    assert_eq!(admin.delete(&format!("{url}/{friend_id}")).await.status, StatusCode::OK);
    assert_eq!(admin.delete(&format!("{url}/{friend_id}")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(admin.get(&url).await.json(), json!({"items": []}));
}

#[tokio::test]
async fn friendship_endpoints_are_admin_only() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (_friend, friend_id) = app.register("friend@example.com").await;
    let url = format!("/api/admin/users/{id}/friends");
    assert_eq!(client.post_json(&url, json!({"friendId": friend_id})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(client.get(&url).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn deleting_an_account_removes_its_friendships() {
    let app = TestApp::new().await;
    let admin = app.admin().await;
    let (_client, id) = app.player().await;
    let (_friend, friend_id) = app.register("friend@example.com").await;
    app.befriend(id, friend_id).await;
    admin.delete(&format!("/api/admin/users/{id}")).await;
    assert_eq!(admin.get(&format!("/api/admin/users/{friend_id}/friends")).await.json(), json!({"items": []}));
}

#[tokio::test]
async fn a_request_is_sent_seen_and_accepted() {
    let app = TestApp::new().await;
    let (alice, alice_id, bob, bob_id) = pair(&app).await;
    let response = ask(&alice, bob_id).await;
    assert!(response.status == StatusCode::CREATED && response.json()["accepted"] == false);

    let (sent, received) = (overview(&alice).await, overview(&bob).await);
    assert!(sent["outgoing"][0]["to"]["id"] == bob_id && sent["incoming"] == json!([]));
    assert_eq!(received["incoming"][0]["from"]["id"], alice_id);
    assert_eq!(bob.get("/api/friends/summary").await.json(), json!({"incoming": 1}));
    assert_eq!(alice.get("/api/friends/summary").await.json(), json!({"incoming": 0}));

    let accepted = accept_first(&bob).await;
    assert!(accepted.status == StatusCode::OK && accepted.json()["gemsAwarded"] == 5);
    assert_eq!(friend_ids(&overview(&alice).await), [bob_id]);
    assert_eq!(friend_ids(&overview(&bob).await), [alice_id]);
    assert!(overview(&bob).await["incoming"] == json!([]) && overview(&alice).await["outgoing"] == json!([]));
}

#[tokio::test]
async fn neighbours_appear_in_the_game_once_accepted() {
    let app = TestApp::new().await;
    let (alice, _alice_id, bob, bob_id) = pair(&app).await;
    ask(&alice, bob_id).await;
    assert_eq!(alice.player_info(None).await.signed_json()["neighbors"], json!([]));

    accept_first(&bob).await;
    assert_eq!(alice.player_info(None).await.signed_json()["neighbors"][0]["pid"], bob_id.to_string());
    assert_eq!(alice.get("/api/game/bootstrap").await.json()["flashVars"]["fb_sig_friends"], bob_id.to_string());
}

#[tokio::test]
async fn only_the_addressee_can_answer_a_request() {
    let app = TestApp::new().await;
    let (alice, _alice_id, bob, bob_id) = pair(&app).await;
    let (carol, _) = app.register("carol@example.com").await;
    ask(&alice, bob_id).await;
    let request_id = first_incoming(&bob).await;

    let accept = format!("/api/friends/requests/{request_id}/accept");
    let decline = format!("/api/friends/requests/{request_id}/decline");
    let cancel = format!("/api/friends/requests/{request_id}");
    assert_eq!(alice.post_empty(&accept).await.status, StatusCode::NOT_FOUND);
    assert_eq!(carol.post_empty(&accept).await.status, StatusCode::NOT_FOUND);
    assert_eq!(carol.post_empty(&decline).await.status, StatusCode::NOT_FOUND);
    assert_eq!(carol.delete(&cancel).await.status, StatusCode::NOT_FOUND);
    assert_eq!(bob.delete(&cancel).await.status, StatusCode::NOT_FOUND);
    assert_eq!(overview(&bob).await["friends"], json!([]));
}

#[tokio::test]
async fn declining_and_cancelling() {
    let app = TestApp::new().await;
    let (alice, alice_id, bob, bob_id) = pair(&app).await;
    ask(&alice, bob_id).await;
    let request_id = first_incoming(&bob).await;
    assert_eq!(bob.post_empty(&format!("/api/friends/requests/{request_id}/decline")).await.status, StatusCode::OK);
    assert!(overview(&bob).await["incoming"] == json!([]) && overview(&alice).await["outgoing"] == json!([]));
    assert_eq!(bob.post_empty(&format!("/api/friends/requests/{request_id}/accept")).await.status, StatusCode::NOT_FOUND);

    assert_eq!(ask(&alice, bob_id).await.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(ask(&bob, alice_id).await.status, StatusCode::CREATED);

    let request = first_incoming(&alice).await;
    assert_eq!(bob.delete(&format!("/api/friends/requests/{request}")).await.status, StatusCode::OK);
    assert_eq!(overview(&alice).await["incoming"], json!([]));
}

#[tokio::test]
async fn a_declined_request_can_be_sent_again_after_the_cooldown() {
    let app = TestApp::new().await;
    let (alice, _alice_id, bob, bob_id) = pair(&app).await;
    ask(&alice, bob_id).await;
    let request_id = first_incoming(&bob).await;
    bob.post_empty(&format!("/api/friends/requests/{request_id}/decline")).await;

    let mut row = app.state.accounts.get_friendship(request_id).await.unwrap().unwrap();
    assert_eq!(row.status, FRIEND_DECLINED);
    row.responded_at = row.responded_at.map(|at| at - chrono::Duration::days(8));
    app.state.accounts.update_friendship(&row).await.unwrap();
    assert_eq!(ask(&alice, bob_id).await.status, StatusCode::CREATED);
}

#[tokio::test]
async fn requests_that_meet_become_a_friendship() {
    let app = TestApp::new().await;
    let (alice, alice_id, bob, bob_id) = pair(&app).await;
    ask(&alice, bob_id).await;
    let response = ask(&bob, alice_id).await;
    assert!(response.status == StatusCode::CREATED && response.json()["accepted"] == true);
    assert_eq!(response.json()["gemsAwarded"], 5);
    assert_eq!(friend_ids(&overview(&alice).await), [bob_id]);
}

#[tokio::test]
async fn requests_are_validated() {
    let app = TestApp::new().await;
    let (alice, alice_id, _bob, bob_id) = pair(&app).await;
    assert_eq!(ask(&alice, alice_id).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(ask(&alice, 999_999_999).await.status, StatusCode::NOT_FOUND);
    assert_eq!(alice.post_json("/api/friends/requests", json!({"playerId": "x"})).await.status, StatusCode::BAD_REQUEST);
    let extra = alice.post_json("/api/friends/requests", json!({"playerId": bob_id, "extra": 1})).await;
    assert_eq!(extra.status, StatusCode::BAD_REQUEST);
    assert_eq!(alice.post_json("/api/friends/requests", json!({})).await.status, StatusCode::BAD_REQUEST);

    assert_eq!(ask(&alice, bob_id).await.status, StatusCode::CREATED);
    assert_eq!(ask(&alice, bob_id).await.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_friend_cannot_be_asked_again_and_can_be_removed() {
    let app = TestApp::new().await;
    let (alice, alice_id, bob, bob_id) = pair(&app).await;
    ask(&alice, bob_id).await;
    accept_first(&bob).await;
    assert_eq!(ask(&alice, bob_id).await.status, StatusCode::CONFLICT);
    assert_eq!(ask(&bob, alice_id).await.status, StatusCode::CONFLICT);

    assert_eq!(bob.delete(&format!("/api/friends/{alice_id}")).await.status, StatusCode::OK);
    assert!(overview(&alice).await["friends"] == json!([]) && overview(&bob).await["friends"] == json!([]));
    assert_eq!(bob.delete(&format!("/api/friends/{alice_id}")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(alice.player_info(Some(bob_id)).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn disabled_accounts_cannot_be_asked() {
    let app = TestApp::new().await;
    let admin = app.admin().await;
    let (alice, _alice_id, _bob, bob_id) = pair(&app).await;
    admin.patch_json(&format!("/api/admin/users/{bob_id}"), json!({"isActive": false})).await;
    assert_eq!(ask(&alice, bob_id).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_friends_api_never_shows_emails() {
    let app = TestApp::new().await;
    let (alice, _alice_id, bob, bob_id) = pair(&app).await;
    ask(&alice, bob_id).await;
    assert!(!alice.get("/api/friends").await.text().contains("bob@example.com"));
    assert!(!bob.get("/api/friends").await.text().contains("alice@example.com"));
}

#[tokio::test]
async fn the_friends_api_requires_a_login() {
    let app = TestApp::new().await;
    let client = app.client();
    for path in ["/api/friends", "/api/friends/summary"] {
        assert_eq!(client.get(path).await.status, StatusCode::UNAUTHORIZED, "{path}");
    }
    assert_eq!(client.post_json("/api/friends/requests", json!({})).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(client.post_empty("/api/friends/requests/1/accept").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(client.delete("/api/friends/1").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sending_requests_is_rate_limited() {
    let app = TestApp::new().await;
    let (alice, _alice_id, _bob, _bob_id) = pair(&app).await;
    let mut statuses = Vec::new();
    for i in 0..14 {
        statuses.push(ask(&alice, 900_000_000 + i).await.status);
    }
    assert!(statuses.contains(&StatusCode::TOO_MANY_REQUESTS));
}

#[tokio::test]
async fn the_friend_list_limit_is_enforced() {
    let app = TestApp::with_env(&[("MAX_FRIENDS", "1")]).await;
    let (alice, _alice_id, bob, bob_id) = pair(&app).await;
    let (carol, carol_id) = app.register("carol@example.com").await;
    ask(&alice, bob_id).await;
    accept_first(&bob).await;
    assert_eq!(ask(&alice, carol_id).await.status, StatusCode::CONFLICT);
    assert_eq!(ask(&carol, bob_id).await.status, StatusCode::CREATED);
    assert_eq!(accept_first(&bob).await.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn the_defaults_are_20_friends_and_5_pending_requests() {
    let app = TestApp::new().await;
    let (alice, ..) = pair(&app).await;
    let limits = overview(&alice).await["limits"].clone();
    assert!(limits["maxFriends"] == 20 && limits["maxPendingRequests"] == 5);
}

#[tokio::test]
async fn at_most_5_sent_requests_can_wait() {
    let app = TestApp::new().await;
    let (alice, _alice_id, _bob, bob_id) = pair(&app).await;
    let mut others = Vec::new();
    for i in 0..5 {
        others.push(app.register(&format!("o{i}@example.com")).await.1);
    }
    assert_eq!(ask(&alice, bob_id).await.status, StatusCode::CREATED);
    for other in &others[..4] {
        assert_eq!(ask(&alice, *other).await.status, StatusCode::CREATED);
    }
    let blocked = ask(&alice, others[4]).await;
    assert_eq!(blocked.status, StatusCode::CONFLICT);
    assert!(blocked.json()["error"]["message"].as_str().unwrap().contains("at most 5"));

    let request_id = overview(&alice).await["outgoing"][0]["requestId"].as_i64().unwrap();
    assert_eq!(alice.delete(&format!("/api/friends/requests/{request_id}")).await.status, StatusCode::OK);
    assert_eq!(ask(&alice, others[4]).await.status, StatusCode::CREATED);
}

#[tokio::test]
async fn the_limits_can_be_changed_with_environment_variables() {
    let app = TestApp::with_env(&[("MAX_FRIENDS", "3"), ("MAX_PENDING_REQUESTS", "2")]).await;
    let (alice, _alice_id, _bob, bob_id) = pair(&app).await;
    let limits = overview(&alice).await["limits"].clone();
    assert!(limits["maxFriends"] == 3 && limits["maxPendingRequests"] == 2);
    let mut others = Vec::new();
    for i in 0..2 {
        others.push(app.register(&format!("x{i}@example.com")).await.1);
    }
    assert_eq!(ask(&alice, bob_id).await.status, StatusCode::CREATED);
    assert_eq!(ask(&alice, others[0]).await.status, StatusCode::CREATED);
    assert_eq!(ask(&alice, others[1]).await.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn both_players_get_5_gems_for_a_new_friend() {
    let app = TestApp::new().await;
    let (alice, alice_id, bob, bob_id) = pair(&app).await;
    let (start_alice, start_bob) = (gems(&app, alice_id).await, gems(&app, bob_id).await);
    ask(&alice, bob_id).await;
    assert!(gems(&app, alice_id).await == start_alice && gems(&app, bob_id).await == start_bob);
    accept_first(&bob).await;
    assert!(gems(&app, alice_id).await == start_alice + 5 && gems(&app, bob_id).await == start_bob + 5);
}

#[tokio::test]
async fn a_friend_pays_only_once() {
    let app = TestApp::new().await;
    let (alice, alice_id, bob, bob_id) = pair(&app).await;
    let start = gems(&app, alice_id).await;
    ask(&alice, bob_id).await;
    accept_first(&bob).await;
    bob.delete(&format!("/api/friends/{alice_id}")).await;
    ask(&alice, bob_id).await;
    accept_first(&bob).await;
    assert_eq!(gems(&app, alice_id).await, start + 5);
}

#[tokio::test]
async fn only_the_first_five_friends_pay() {
    let app = TestApp::new().await;
    let (main, main_id) = app.register("main@example.com").await;
    let start = gems(&app, main_id).await;
    for i in 0..7 {
        let (other, _) = app.register(&format!("p{i}@example.com")).await;
        ask(&other, main_id).await;
        accept_first(&main).await;
    }
    assert_eq!(gems(&app, main_id).await, start + 5 * FRIEND_REWARD_LIMIT as i64);
    assert_eq!(overview(&main).await["friends"].as_array().unwrap().len(), 7);
}

#[tokio::test]
async fn friendships_made_by_an_admin_also_pay() {
    let app = TestApp::new().await;
    let (_alice, alice_id, _bob, bob_id) = pair(&app).await;
    let admin = app.admin().await;
    let start = gems(&app, alice_id).await;
    let added = admin.post_json(&format!("/api/admin/users/{alice_id}/friends"), json!({"friendId": bob_id})).await;
    assert_eq!(added.status, StatusCode::CREATED);
    assert_eq!(gems(&app, alice_id).await, start + 5);
}

#[tokio::test]
async fn an_admin_can_accept_a_pending_request_by_adding_the_friend() {
    let app = TestApp::new().await;
    let (alice, alice_id, bob, bob_id) = pair(&app).await;
    let admin = app.admin().await;
    ask(&alice, bob_id).await;
    let added = admin.post_json(&format!("/api/admin/users/{alice_id}/friends"), json!({"friendId": bob_id})).await;
    assert_eq!(added.status, StatusCode::CREATED);
    assert_eq!(overview(&bob).await["incoming"], json!([]));
    assert_eq!(friend_ids(&overview(&bob).await), [alice_id]);
}

const ALLIES_MARKET: i64 = 79;
const OTHER_BUILDING: i64 = 16;

#[tokio::test]
async fn friends_are_assigned_as_workers_and_nothing_is_paid() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (_friend, friend_id) = app.register("ally@example.com").await;
    app.befriend(id, friend_id).await;
    app.add_item(id, 50, ALLIES_MARKET, json!({})).await;
    let before = app.info(id).await;

    client.packet(&[("set_resource_allies", json!([50, format!("[{friend_id}]")]))]).await;
    assert_eq!(app.save(id).await.unwrap()["map"]["items"]["50"][6]["allies"], json!([friend_id]));
    assert_eq!(app.info(id).await, before);

    client.packet(&[("set_resource_allies", json!([50, "[]"]))]).await;
    assert_eq!(app.save(id).await.unwrap()["map"]["items"]["50"][6]["allies"], json!([]));
}

#[tokio::test]
async fn workers_must_be_friends_of_a_real_allies_market() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (_stranger, stranger_id) = app.register("stranger@example.com").await;
    app.add_item(id, 50, ALLIES_MARKET, json!({})).await;
    app.add_item(id, 51, OTHER_BUILDING, json!({})).await;
    let before = app.save(id).await.unwrap();

    client.packet(&[("set_resource_allies", json!([50, format!("[{stranger_id}]")]))]).await;
    client.packet(&[("set_resource_allies", json!([51, "[]"]))]).await;
    assert_eq!(app.save(id).await.unwrap()["map"], before["map"]);
}

#[tokio::test]
async fn the_allies_market_has_six_slots() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let mut ids = Vec::new();
    for i in 0..7 {
        let ally = app.register(&format!("ally{i}@example.com")).await.1;
        app.befriend(id, ally).await;
        ids.push(ally);
    }
    app.add_item(id, 50, ALLIES_MARKET, json!({})).await;
    let list = serde_json::to_string(&ids).unwrap();
    client.packet(&[("set_resource_allies", json!([50, list]))]).await;
    assert!(app.save(id).await.unwrap()["map"]["items"]["50"][6].get("allies").is_none());
}

const SMALL_MISTERY_EGG: i64 = 154;
const NORMAL_DRAGON_EGG: i64 = 1000;

async fn add_egg(app: &TestApp, id: i64, egg_uid: i64, item_id: i64) {
    app.edit_save(id, |data| {
        data["map"]["eggs"].as_array_mut().unwrap().push(
            json!({"id": egg_uid, "type": item_id, "name": "Egg", "xp": 0, "hatched": false, "ts": 0}),
        );
    })
    .await;
}

#[tokio::test]
async fn hatching_gives_exactly_one_of_the_tiers_configured_rewards() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    add_egg(&app, id, 9001, SMALL_MISTERY_EGG).await;
    let before = app.info(id).await;

    client.packet(&[("mistery_egg_hatch", json!([9001]))]).await;
    let save = app.save(id).await.unwrap();
    assert!(save["map"]["eggs"].as_array().unwrap().iter().all(|e| e["id"] != 9001));

    let table = app.state.storage.game.config.section("mistery_egg")["SMALL"]["rewards"].clone();
    let rewards = table.as_array().unwrap();
    let dragon_ids: Vec<&Value> = rewards.iter().filter(|r| r.is_number()).collect();
    let after = &save["playerInfo"];
    let gained_dragon = save["map"]["store"]["dragons"].as_array().unwrap().iter().any(|d| dragon_ids.contains(&&d["iid"]));
    let gained_resources = rewards.iter().filter(|r| r.is_object()).any(|r| {
        after["gold"].as_i64().unwrap() - before["gold"].as_i64().unwrap() == r["g"].as_i64().unwrap_or(0)
            && after["food"].as_i64().unwrap() - before["food"].as_i64().unwrap() == r["f"].as_i64().unwrap_or(0)
    });
    assert!(gained_dragon || gained_resources);
}

#[tokio::test]
async fn a_mistery_egg_rejects_unknown_or_normal_eggs_and_double_hatching() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    client.packet(&[("mistery_egg_hatch", json!([12345]))]).await;
    assert_eq!(app.save(id).await.unwrap()["map"]["store"]["dragons"], json!([]));

    add_egg(&app, id, 9002, NORMAL_DRAGON_EGG).await;
    client.packet(&[("mistery_egg_hatch", json!([9002]))]).await;
    let save = app.save(id).await.unwrap();
    assert!(save["map"]["eggs"].as_array().unwrap().iter().any(|e| e["id"] == 9002));
    assert_eq!(save["map"]["store"]["dragons"], json!([]));

    add_egg(&app, id, 9003, SMALL_MISTERY_EGG).await;
    client.packet(&[("mistery_egg_hatch", json!([9003]))]).await;
    let after_first = app.save(id).await.unwrap();
    client.packet(&[("mistery_egg_hatch", json!([9003]))]).await;
    let mut again = app.save(id).await.unwrap();
    let mut first = after_first;
    first["timestamp"] = json!(0);
    again["timestamp"] = json!(0);
    assert_eq!(first, again);
}
