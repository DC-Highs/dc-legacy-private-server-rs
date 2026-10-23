//! The client is untrusted: every way of injecting resources or breaking the save must fail.
mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::{json, Value};

const FARM: i64 = 16;
const HABITAT: i64 = 24;
const DRAGON: i64 = 1000;

fn now() -> i64 {
    dragon_city::util::now_ts()
}

fn first_result(responses: &[Value]) -> Value {
    responses[0]["result"].clone()
}

#[tokio::test]
async fn game_endpoints_require_a_login() {
    let app = TestApp::new().await;
    let client = app.client();
    let post = client.post_packet_raw("/static/dragoncity/srv/packet.php", "{}", &[]).await;
    assert_eq!(post.status, StatusCode::UNAUTHORIZED);
    for path in [
        "/static/dragoncity/srv/get_player_info.json",
        "/static/dragoncity/srv/get_game_config.json",
        "/flash/DCLoader.swf",
        "/stream",
    ] {
        assert_eq!(client.get(path).await.status, StatusCode::UNAUTHORIZED, "{path}");
    }
}

#[tokio::test]
async fn the_userid_sent_by_the_client_is_ignored() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (_other, other_id) = app.register("other@example.com").await;
    app.give(other_id, json!({"gold": 5000})).await;
    app.give(id, json!({"gold": 5000})).await;

    let payload = format!("sig;{}", json!({"commands": [{"cmd": "buy", "args": ["700", FARM, 3, 3, 0, 0, 1], "number": 1}]}));
    let body = form_urlencoded::Serializer::new(String::new())
        .append_pair("data", &payload)
        .append_pair("id", &other_id.to_string())
        .finish();
    let response = client
        .send(
            Method::POST,
            &format!("/static/dragoncity/srv/packet.php?USERID={other_id}"),
            &[],
            Some("application/x-www-form-urlencoded"),
            body.into_bytes(),
        )
        .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(app.gold(other_id).await, 5000);
    assert_eq!(app.gold(id).await, 4900);
}

#[tokio::test]
async fn visiting_an_unknown_neighbour_does_not_create_a_save() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    assert_eq!(client.player_info(Some(999_999_999)).await.status, StatusCode::NOT_FOUND);
    assert!(!app.state.storage.player_exists(999_999_999).await.unwrap());
}

#[tokio::test]
async fn visiting_a_real_neighbour_is_read_only() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (_other, other_id) = app.register("other@example.com").await;
    app.befriend(id, other_id).await;
    let before = app.save(other_id).await.unwrap();
    assert_eq!(client.player_info(Some(other_id)).await.status, StatusCode::OK);
    assert_eq!(app.save(other_id).await.unwrap()["playerInfo"], before["playerInfo"]);
}

#[tokio::test]
async fn buy_needs_the_money_and_a_free_slot() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 50})).await;
    client.packet(&[("buy", json!(["700", FARM, 3, 3, 0, 0, 1]))]).await;
    assert!(app.save(id).await.unwrap()["map"]["items"].get("700").is_none());
    assert_eq!(app.gold(id).await, 50);

    app.give(id, json!({"gold": 1000})).await;
    client.packet(&[("buy", json!(["700", FARM, 3, 3, 0, 0, 1]))]).await;
    assert!(app.save(id).await.unwrap()["map"]["items"].get("700").is_some());
    assert_eq!(app.gold(id).await, 900);

    client.packet(&[("buy", json!(["700", FARM, 9, 9, 0, 0, 1]))]).await;
    assert_eq!(app.gold(id).await, 900);
}

#[tokio::test]
async fn free_or_unsellable_items_cannot_be_bought() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let free_item = app
        .state
        .storage
        .game
        .config
        .items()
        .iter()
        .find(|i| !dragon_city::json::truthy(&i["costs"]) && i["group_type"] != "DRAGON")
        .map(|i| i["id"].as_i64().unwrap())
        .unwrap();
    app.give(id, json!({"gold": 1_000_000})).await;
    client
        .packet(&[
            ("buy", json!(["701", free_item, 3, 3, 0, 0, 1])),
            ("buy", json!(["702", DRAGON, 3, 3, 0, 0, 1])),
        ])
        .await;
    let items = app.save(id).await.unwrap()["map"]["items"].clone();
    assert!(items.get("701").is_none() && items.get("702").is_none());
    assert_eq!(app.gold(id).await, 1_000_000);
}

#[tokio::test]
async fn level_requirements_are_enforced() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 1_000_000})).await;
    client.packet(&[("buy", json!(["703", HABITAT, 3, 3, 0, 0, 1]))]).await;
    assert!(app.save(id).await.unwrap()["map"]["items"].get("703").is_none());
    app.give(id, json!({"level": 7})).await;
    client.packet(&[("buy", json!(["703", HABITAT, 3, 3, 0, 0, 1]))]).await;
    assert!(app.save(id).await.unwrap()["map"]["items"].get("703").is_some());
}

#[tokio::test]
async fn buy_egg_checks_funds_and_that_the_item_is_a_dragon() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 100})).await;
    client.packet(&[("buy_egg", json!([5001, DRAGON]))]).await;
    assert_eq!(app.save(id).await.unwrap()["map"]["eggs"], json!([]));

    app.give(id, json!({"gold": 20000})).await;
    client
        .packet(&[
            ("buy_egg", json!([5001, DRAGON])),
            ("buy_egg", json!([5001, DRAGON])),
            ("buy_egg", json!([5002, FARM])),
        ])
        .await;
    let eggs = app.save(id).await.unwrap()["map"]["eggs"].clone();
    assert_eq!(eggs.as_array().unwrap().iter().map(|e| e["id"].clone()).collect::<Vec<_>>(), [json!(5001)]);
    assert_eq!(app.gold(id).await, 5000);
}

#[tokio::test]
async fn absurd_numbers_are_rejected_and_do_not_poison_the_save() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 1_000_000})).await;
    client
        .packet(&[
            ("buy", json!(["704", FARM, 1e30, 3, 0, 0, 1])),
            ("buy", json!(["705", FARM, -5, 3, 0, 0, 1])),
            ("buy", json!(["706", FARM, "3; DROP TABLE", 3, 0, 0, 1])),
            ("buy", json!([true, FARM, 3, 3, 0, 0, 1])),
            ("buy", json!(["707", FARM, {"$gt": 1}, 3, 0, 0, 1])),
        ])
        .await;
    assert_eq!(app.gold(id).await, 1_000_000);
    assert_eq!(first_result(&client.packet(&[("ping", json!([]))]).await), "ok");
}

#[tokio::test]
async fn huge_indexes_cannot_bloat_the_save() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    client
        .packet(&[
            ("set_goals", json!([1_000_000_000i64, "[1]"])),
            ("set_goals", json!([5, "[1]"])),
            ("start_training", json!(["1", 1, 1, 1_000_000])),
        ])
        .await;
    let goals = app.save(id).await.unwrap()["privateState"]["goals"].clone();
    assert!(goals.as_array().unwrap().len() < 1000);
}

#[tokio::test]
async fn nan_and_oversized_packets_are_refused() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    let path = "/static/dragoncity/srv/packet.php";
    assert_eq!(client.post_packet_raw(path, "{\"commands\": [NaN]}", &[]).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(client.post_packet_raw(path, "not json", &[]).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(client.post_packet_raw(path, "[1]", &[]).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(client.post_packet_raw(path, &"x".repeat(300_000), &[]).await.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn a_packet_is_capped_to_200_commands() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 1_000_000})).await;
    let commands: Vec<(&str, Value)> = (0..1000).map(|i| ("sell_egg", json!([i]))).collect();
    assert_eq!(client.packet(&commands).await.len(), 200);
}

#[tokio::test]
async fn unknown_commands_are_answered_with_ok() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    let results = client.packet(&[("fly_to_the_moon", json!([1]))]).await;
    assert_eq!(results[0]["result"], "ok");
    assert_eq!(results[0]["cmd"], "fly_to_the_moon");
    assert_eq!(results[0]["number"], 1);
    assert!(results[0]["resources"]["pu"]["gold"].as_i64().unwrap() >= 0);
}

#[tokio::test]
async fn a_failing_command_leaves_no_partial_changes() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.add_item(id, 900, HABITAT, json!({})).await;
    app.edit_save(id, |data| data["privateState"]["dragonarium"] = json!([[5], [], [], [], []])).await;
    let before = app.save(id).await.unwrap();

    let results = client.packet(&[("dragonarium_pull", json!([0, 0, 0, 900]))]).await;
    assert_eq!(results[0]["result"], "ok");
    let after = app.save(id).await.unwrap();
    assert_eq!(after["privateState"]["dragonarium"], before["privateState"]["dragonarium"]);
    assert_eq!(after["map"]["dragons"], before["map"]["dragons"]);
}

#[tokio::test]
async fn levels_go_up_one_at_a_time() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"xp": 1_000_000})).await;
    client.packet(&[("level_up", json!([10]))]).await;
    assert_eq!(app.info(id).await["level"], 1);
    client.packet(&[("level_up", json!([2])), ("level_up", json!([2])), ("level_up", json!([4]))]).await;
    assert_eq!(app.info(id).await["level"], 2);
}

#[tokio::test]
async fn a_level_needs_its_experience() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let cash = app.info(id).await["cash"].as_i64().unwrap();
    client.packet(&[("level_up", json!([2]))]).await;
    assert_eq!(app.info(id).await["level"], 1);
    assert_eq!(app.info(id).await["cash"], cash);
    app.give(id, json!({"xp": 200})).await;
    client.packet(&[("level_up", json!([2]))]).await;
    assert_eq!(app.info(id).await["level"], 2);
    assert_eq!(app.info(id).await["cash"], cash + 1);
}

#[tokio::test]
async fn reaching_level_5_opens_the_black_market() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"level": 4, "xp": 10_000_000})).await;
    client.packet(&[("level_up", json!([5]))]).await;
    let private = app.save(id).await.unwrap()["privateState"].clone();
    assert!(private["BlackMarketStart"].as_i64().unwrap() > now());
    assert_eq!(private["kompuBoxesDragons"].as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn a_goal_pays_out_once() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let before = app.gold(id).await;
    client.packet(&[("complete_goal", json!([2]))]).await;
    let after_first = app.gold(id).await;
    assert!(after_first > before);
    client.packet(&[("complete_goal", json!([2])), ("complete_goal", json!([2]))]).await;
    assert_eq!(app.gold(id).await, after_first);
    assert_eq!(app.save(id).await.unwrap()["privateState"]["achievement_points"], 35);
}

#[tokio::test]
async fn building_xp_is_paid_once() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.add_item(id, 800, 9, json!({"tb": now() + 100})).await;
    client.packet(&[("finish_building", json!([800, 0]))]).await;
    let xp = app.info(id).await["xp"].as_i64().unwrap();
    assert!(xp > 0);
    client.packet(&[("finish_building", json!([800, 0])), ("finish_building", json!([800, 0]))]).await;
    assert_eq!(app.info(id).await["xp"], xp);
}

#[tokio::test]
async fn the_weekly_reward_is_claimed_once_per_week() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    client.packet(&[("weekly_reward", json!(["[0]"]))]).await;
    let after_first = app.gold(id).await;
    client.packet(&[("weekly_reward", json!(["[0]"]))]).await;
    assert_eq!(app.gold(id).await, after_first);
}

async fn set_pending_prize(app: &TestApp, id: i64, prize: i64) {
    app.edit_save(id, |data| data["privateState"]["deus_cards"] = json!({"pending_prize": prize})).await;
}

#[tokio::test]
async fn free_card_draws_are_limited() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let first = first_result(&client.packet(&[("deus_cards_generate", json!([0, 0]))]).await);
    assert!((0..9).contains(&first.as_i64().unwrap()));
    let second = first_result(&client.packet(&[("deus_cards_generate", json!([0, 0]))]).await);
    assert_eq!(second, "ok");
    assert_eq!(app.save(id).await.unwrap()["privateState"]["deus_cards"]["pending_prize"], first);

    set_pending_prize(&app, id, 0).await;
    let before = app.gold(id).await;
    client.packet(&[("deus_cards_claim", json!([]))]).await;
    let gold_after = app.gold(id).await;
    assert_eq!(gold_after, before + 5000);

    client
        .packet(&[("deus_cards_claim", json!([])), ("deus_cards_generate", json!([0, 0])), ("deus_cards_claim", json!([]))])
        .await;
    assert_eq!(app.gold(id).await, gold_after);
}

#[tokio::test]
async fn the_retry_prize_gives_one_more_free_draw() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    client.packet(&[("deus_cards_generate", json!([0, 0]))]).await;
    set_pending_prize(&app, id, 2).await;
    client.packet(&[("deus_cards_claim", json!([]))]).await;

    let before = app.gold(id).await;
    client.packet(&[("deus_cards_generate", json!([0, 0]))]).await;
    set_pending_prize(&app, id, 0).await;
    client.packet(&[("deus_cards_claim", json!([]))]).await;
    assert_eq!(app.gold(id).await, before + 5000);

    client.packet(&[("deus_cards_generate", json!([0, 0]))]).await;
    assert_eq!(app.save(id).await.unwrap()["privateState"]["deus_cards"]["pending_prize"], Value::Null);
}

#[tokio::test]
async fn a_card_prize_cannot_be_claimed_without_a_draw() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let before = app.gold(id).await;
    client.packet(&[("deus_cards_claim", json!([]))]).await;
    assert_eq!(app.gold(id).await, before);
}

#[tokio::test]
async fn habitat_gold_is_bounded_by_what_the_dragons_produced() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.add_item(id, 810, HABITAT, json!({"lat": now()})).await;
    let before = app.gold(id).await;
    client.packet(&[("collect", json!([810, 1_000_000_000]))]).await;
    assert!(app.gold(id).await - before <= 100);
}

#[tokio::test]
async fn only_habitats_pay_gold() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.add_item(id, 811, FARM, json!({})).await;
    let before = app.gold(id).await;
    client.packet(&[("collect", json!([811, 5000]))]).await;
    assert_eq!(app.gold(id).await, before);
}

#[tokio::test]
async fn a_crop_cannot_be_harvested_before_it_grows() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 1_000_000, "completed_tutorial": "1"})).await;
    app.add_item(id, 820, FARM, json!({})).await;
    client.packet(&[("activate", json!([820, 5]))]).await;
    let food = app.info(id).await["food"].as_i64().unwrap();
    client.packet(&[("collect", json!([820, "fc"]))]).await;
    assert_eq!(app.info(id).await["food"], food);
    assert_eq!(app.save(id).await.unwrap()["map"]["items"]["820"][6]["cp"], 5);

    app.give(id, json!({"cash": 1000})).await;
    client.packet(&[("speed_collect", json!([820, 1])), ("collect", json!([820, "fc"]))]).await;
    assert!(app.info(id).await["food"].as_i64().unwrap() > food);
}

#[tokio::test]
async fn the_tutorial_may_harvest_one_crop_right_away() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 1_000_000})).await;
    app.add_item(id, 822, FARM, json!({})).await;
    let food = app.info(id).await["food"].as_i64().unwrap();
    client.packet(&[("activate", json!([822, 1])), ("collect", json!([822, "fc"]))]).await;
    let harvested = app.info(id).await["food"].as_i64().unwrap();
    assert!(harvested > food);
    client.packet(&[("activate", json!([822, 1])), ("collect", json!([822, "fc"]))]).await;
    assert_eq!(app.info(id).await["food"], harvested);
}

#[tokio::test]
async fn farm_activation_needs_the_money() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 0})).await;
    app.add_item(id, 821, FARM, json!({})).await;
    client.packet(&[("activate", json!([821, 3]))]).await;
    assert!(app.save(id).await.unwrap()["map"]["items"]["821"][6].get("cp").is_none());
}

#[tokio::test]
async fn dragons_cannot_be_conjured_from_nothing() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.add_item(id, 830, HABITAT, json!({})).await;
    client
        .packet(&[
            ("place_stored_dragon", json!([9001, "830", "Rex", 1041, 40, 0])),
            ("dragonarium_place_stored", json!(["Rex", 1041, 40, 0])),
        ])
        .await;
    let save = app.save(id).await.unwrap();
    assert_eq!(save["map"]["dragons"], json!([]));
    assert_eq!(save["privateState"]["dragonarium"], json!([[], [], [], [], []]));
}

#[tokio::test]
async fn a_stored_dragon_keeps_its_stored_level() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.add_item(id, 830, HABITAT, json!({})).await;
    app.edit_save(id, |data| data["map"]["store"]["dragons"] = json!([{"iid": DRAGON, "lvl": 3}])).await;
    client.packet(&[("place_stored_dragon", json!([9001, "830", "Rex", DRAGON, 40, 0]))]).await;
    assert_eq!(app.save(id).await.unwrap()["map"]["dragons"], json!([]));
    client.packet(&[("place_stored_dragon", json!([9001, "830", "Rex", DRAGON, 3, 0]))]).await;
    let save = app.save(id).await.unwrap();
    assert_eq!(save["map"]["dragons"][0]["xp"], 8);
    assert_eq!(save["map"]["store"]["dragons"], json!([]));
}

#[tokio::test]
async fn ids_chosen_by_the_client_cannot_collide() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 1_000_000})).await;
    client.packet(&[("buy_egg", json!([6000, DRAGON])), ("buy_egg", json!([6000, DRAGON]))]).await;
    assert_eq!(app.save(id).await.unwrap()["map"]["eggs"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn overspending_gems_is_refused() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"cash": 1})).await;
    client
        .packet(&[("dragonarium_add", json!([0])), ("kompu_boxes_new_list", json!([])), ("kompu_boxes_open_box", json!([]))])
        .await;
    let save = app.save(id).await.unwrap();
    assert_eq!(save["playerInfo"]["cash"], 1);
    assert_eq!(save["privateState"]["dragonariumSlot"][0], 2);
}

#[tokio::test]
async fn expansions_are_paid_for() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 0, "cash": 0})).await;
    client.packet(&[("expand_island", json!([90, 1]))]).await;
    let expansions = app.save(id).await.unwrap()["map"]["expansions"].clone();
    assert!(!expansions.as_array().map(|e| e.contains(&json!(90))).unwrap_or(false));
}

fn trainable_attack() -> i64 {
    let app_config = dragon_city::gamedata::GameData::load(std::path::Path::new("web/srv")).unwrap();
    let dragon = app_config.config.item(&json!(DRAGON)).unwrap();
    dragon["trainable_attacks"][0].as_i64().unwrap()
}

fn trainee(id: i64) -> Value {
    json!({"id": id, "type": DRAGON, "xp": 0, "habitatId": 0, "name": "T", "ts": 0, "work_id": [], "attack": []})
}

#[tokio::test]
async fn training_takes_its_time() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let attack = trainable_attack();
    app.add_item(id, 840, 12, json!({})).await;
    app.edit_save(id, |data| data["map"]["dragons"] = json!([trainee(77)])).await;
    client.packet(&[("start_training", json!([840, 77, attack, 0])), ("finish_training", json!([840]))]).await;
    assert!(app.save(id).await.unwrap()["map"]["work"].get("840").is_some());

    app.edit_save(id, |data| data["map"]["work"]["840"][2] = json!(1)).await;
    client.packet(&[("finish_training", json!([840]))]).await;
    let save = app.save(id).await.unwrap();
    assert!(save["map"]["work"].get("840").is_none());
    assert_eq!(save["map"]["dragons"][0]["attack"][0], attack);
}

#[tokio::test]
async fn speed_training_cannot_be_used_for_free() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let attack = trainable_attack();
    app.add_item(id, 841, 12, json!({})).await;
    app.edit_save(id, |data| data["map"]["dragons"] = json!([trainee(78)])).await;
    client.packet(&[("start_training", json!([841, 78, attack, 0]))]).await;
    app.give(id, json!({"cash": 0})).await;
    client.packet(&[("speed_training", json!([841]))]).await;
    assert!(app.save(id).await.unwrap()["map"]["work"].get("841").is_some());
}

#[tokio::test]
async fn assists_are_queued_and_paid_only_for_real_clicks() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (other, other_id) = app.register("other@example.com").await;
    app.befriend(id, other_id).await;
    client.packet(&[("assist_receive", json!([FARM]))]).await;
    assert_eq!(app.info(id).await["xp"], 0);

    other
        .packet(&[
            ("assist_neighbor", json!([id, "[1,2,3]"])),
            ("assist_neighbor", json!([other_id, "[1]"])),
            ("assist_neighbor", json!([123_456_789, "[1]"])),
        ])
        .await;
    client.packet(&[("ping", json!([]))]).await;
    let save = app.save(id).await.unwrap();
    assert_eq!(save["map"]["receivedAssists"][format!("u{other_id}")], json!([1, 2, 3]));
    assert_eq!(save["privateState"]["assistRewardsLeft"], 3);

    let claims: Vec<(&str, Value)> = (0..5).map(|_| ("assist_receive", json!([FARM]))).collect();
    client.packet(&claims).await;
    assert_eq!(app.info(id).await["xp"], 30);
}

#[tokio::test]
async fn assists_have_a_size_limit() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let (other, other_id) = app.register("other@example.com").await;
    app.befriend(id, other_id).await;
    let clicks = serde_json::to_string(&(0..1000).collect::<Vec<i32>>()).unwrap();
    other.packet(&[("assist_neighbor", json!([id, clicks]))]).await;
    client.packet(&[("ping", json!([]))]).await;
    assert!(app.save(id).await.unwrap()["privateState"].get("assistRewardsLeft").is_none());
}

#[tokio::test]
async fn received_assists_can_be_cleaned() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.edit_save(id, |data| data["map"]["receivedAssists"] = json!({"u5": [1], "u6": [2]})).await;
    client.packet(&[("clean_received_assists", json!([5]))]).await;
    assert_eq!(app.save(id).await.unwrap()["map"]["receivedAssists"], json!({"u6": [2]}));
}

#[tokio::test]
async fn pvp_results_need_a_real_fight() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    client.packet(&[("end_attack_f", json!(["100000050", 1, 1, ""]))]).await;
    assert_eq!(app.save(id).await.unwrap()["privateState"]["achievement_points"], 0);
    assert!(app.info(id).await.get("pvpLevel").map(|l| l == 1).unwrap_or(true));

    let units = first_result(&client.packet(&[("attack_player", json!(["100000050"]))]).await);
    assert!(units.is_array());
    client.packet(&[("attack_player", json!(["424242"]))]).await;
    client.packet(&[("end_attack_f", json!(["100000050", 1, 1, ""]))]).await;
    assert_eq!(app.save(id).await.unwrap()["privateState"]["achievement_points"], 0);

    app.edit_save(id, |data| {
        let time = data["privateState"]["attacksSent"][0]["time"].as_i64().unwrap();
        data["privateState"]["attacksSent"][0]["time"] = json!(time - 60);
    })
    .await;
    client
        .packet(&[("end_attack_f", json!(["100000050", 1, 1, ""])), ("end_attack_f", json!(["100000050", 1, 1, ""]))])
        .await;
    assert_eq!(app.save(id).await.unwrap()["privateState"]["achievement_points"], 35);
}

#[tokio::test]
async fn tournaments_need_to_begin_before_they_end() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    client.packet(&[("end_tournament", json!(["1", "{}"]))]).await;
    assert_eq!(app.save(id).await.unwrap()["privateState"]["currentTournament"], 1);

    client.packet(&[("begin_tournament", json!([]))]).await;
    app.edit_save(id, |data| data["privateState"]["beginTournamentTs"] = json!(now() - 60)).await;
    client.packet(&[("end_tournament", json!(["1", "{}"])), ("end_tournament", json!(["1", "{}"]))]).await;
    assert_eq!(app.save(id).await.unwrap()["privateState"]["currentTournament"], 2);
}

#[tokio::test]
async fn attack_teams_only_use_your_own_dragons() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let teams = |save: &Value| save["privateState"].get("teams").and_then(|t| t.get("pvp")).cloned();
    client.packet(&[("set_attack_team", json!(["pvp", "[1,2,3]"]))]).await;
    assert!(teams(&app.save(id).await.unwrap()).is_none());
    let many = serde_json::to_string(&(0..100).collect::<Vec<i32>>()).unwrap();
    client.packet(&[("set_attack_team", json!(["pvp", many]))]).await;
    assert!(teams(&app.save(id).await.unwrap()).is_none());

    app.edit_save(id, |data| data["map"]["dragons"] = json!([trainee(77)])).await;
    client.packet(&[("set_attack_team", json!(["pvp", "[77]"]))]).await;
    assert_eq!(teams(&app.save(id).await.unwrap()), Some(json!([{"id": 77}])));
}

#[tokio::test]
async fn minigame_rewards_have_a_cooldown_and_a_limit() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let cash = app.info(id).await["cash"].as_i64().unwrap();
    assert_eq!(client.post_json("/minigame/reward", json!({"gems": 3})).await.status, StatusCode::OK);
    assert_eq!(client.post_json("/minigame/reward", json!({"gems": 3})).await.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(client.post_json("/minigame/reward", json!({"gems": 50})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.info(id).await["cash"], cash + 3);
}

#[tokio::test]
async fn the_market_charges_points_and_one_time_offers() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let market = client.get("/market_data").await.json();
    let offer_id = market["offers"][1]["id"].as_i64().unwrap();
    let redeem = |offer: Value| json!({ "offer_id": offer });
    assert_eq!(client.post_json("/market/redeem", redeem(json!(offer_id))).await.status, StatusCode::BAD_REQUEST);
    app.edit_save(id, |data| data["privateState"]["achievement_points"] = json!(1_000_000)).await;
    assert_eq!(client.post_json("/market/redeem", redeem(json!(offer_id))).await.status, StatusCode::OK);
    assert_eq!(client.post_json("/market/redeem", redeem(json!(offer_id))).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(client.post_json("/market/redeem", redeem(json!("1; drop"))).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_player_info_is_signed_and_the_config_is_served() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let info = client.player_info(None).await;
    assert_eq!(info.status, StatusCode::OK);
    assert_eq!(info.signed_json()["playerInfo"]["pid"], id.to_string());

    let config = client.get("/static/dragoncity/srv/get_game_config.json").await;
    assert_eq!(config.headers["cache-control"], "no-store, no-cache, must-revalidate");
    let body = config.signed_json();
    assert!(body["items"].as_array().unwrap().len() > 100);
}

#[tokio::test]
async fn pvp_leagues_are_served_in_pages() {
    let app = TestApp::new().await;
    let (client, _) = app.player().await;
    let page = client.get("/static/dragoncity/srv/get_pvp_isle.json?level=1").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.signed_json().as_array().unwrap().len() <= 8);
    assert_eq!(client.get("/static/dragoncity/srv/get_pvp_isle.json?level=x").await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn game_saved_events_reach_the_stream() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    let mut events = app.state.sse.subscribe(id);
    client.packet(&[("ping", json!([]))]).await;
    assert_eq!(events.recv().await.unwrap(), "JUEGO_GUARDADO");
}

#[tokio::test]
async fn a_normal_session_still_works() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 1_000_000, "cash": 100, "level": 7, "xp": 1_000_000})).await;
    client
        .packet(&[
            ("buy", json!(["900", HABITAT, 10, 10, 0, 0, 1])),
            ("buy", json!(["901", FARM, 12, 12, 0, 0, 1])),
        ])
        .await;
    let items = app.save(id).await.unwrap()["map"]["items"].clone();
    assert!(items["900"][6]["tb"].as_i64().is_some() && items.get("901").is_some());

    client.packet(&[("finish_building", json!(["900", 0]))]).await;
    assert!(app.save(id).await.unwrap()["map"]["items"]["900"][6].get("tb").is_none());

    client.packet(&[("buy_egg", json!([5001, DRAGON])), ("hatch_egg", json!([5001, 0, 900, "Fido"]))]).await;
    let save = app.save(id).await.unwrap();
    assert_eq!(save["map"]["eggs"], json!([]));
    assert_eq!(save["map"]["dragons"][0]["name"], "Fido");
    assert_eq!(save["map"]["ownedDragons"][DRAGON.to_string()], json!([1, 0, 0]));

    client
        .packet(&[
            ("feed_dragon", json!([5001])),
            ("rename_dragon", json!([5001, "Rex"])),
            ("move", json!(["901", 20, 20, 1])),
            ("store_item", json!(["901"])),
            ("place_stored_item", json!(["902", FARM, 4, 4])),
            ("sell", json!(["902"])),
        ])
        .await;
    let save = app.save(id).await.unwrap();
    assert_eq!(save["map"]["dragons"][0]["name"], "Rex");
    assert_eq!(save["map"]["dragons"][0]["xp"], 1);
    assert!(save["map"]["items"].get("901").is_none() && save["map"]["items"].get("902").is_none());

    client.packet(&[("dragonarium_push", json!([5001])), ("dragonarium_pull", json!([0, 0, 0, 900]))]).await;
    let save = app.save(id).await.unwrap();
    assert_eq!(save["map"]["dragons"].as_array().unwrap().len(), 1);
    assert_eq!(save["map"]["dragons"][0]["habitatId"], 900);

    let cave = client.packet(&[("breed_dragon", json!(["900", 5001, 5002, 0]))]).await;
    assert_eq!(cave[0]["result"], "ok");
    client.packet(&[("level_up", json!([8])), ("complete_tutorial", json!([1])), ("sync", json!(["hash"]))]).await;
    assert_eq!(app.info(id).await["level"], 8);
    assert_eq!(app.info(id).await["completed_tutorial"], "1");
}

#[tokio::test]
async fn breeding_produces_an_egg_after_the_wait() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.add_item(id, 950, 16, json!({})).await;
    app.edit_save(id, |data| {
        data["map"]["dragons"] = json!([
            {"id": 11, "type": DRAGON, "xp": 40, "habitatId": 0, "name": "A", "ts": 0, "work_id": [], "attack": []},
            {"id": 12, "type": DRAGON, "xp": 40, "habitatId": 0, "name": "B", "ts": 0, "work_id": [], "attack": []},
        ]);
    })
    .await;
    let result = first_result(&client.packet(&[("breed_dragon", json!(["950", 11, 12, 0]))]).await);
    let offspring = result.as_i64().expect("breeding answers with the dragon type");
    assert!(app.save(id).await.unwrap()["map"]["work"].get("950").is_some());

    let done = first_result(&client.packet(&[("finish_breeding2", json!(["950", 6001, "951"]))]).await);
    assert_eq!(done, offspring);
    let save = app.save(id).await.unwrap();
    assert!(save["map"]["work"].get("950").is_none());
    assert_eq!(save["map"]["eggs"][0]["type"], offspring);
}

#[tokio::test]
async fn speeding_up_an_egg_costs_gems() {
    let app = TestApp::new().await;
    let (client, id) = app.player().await;
    app.give(id, json!({"gold": 1_000_000, "cash": 0})).await;
    client.packet(&[("buy_egg", json!([5001, DRAGON]))]).await;
    client.packet(&[("speed_hatch", json!([5001]))]).await;
    let ts = app.save(id).await.unwrap()["map"]["eggs"][0]["ts"].as_i64().unwrap();
    assert!(ts > now() - 100);

    app.give(id, json!({"cash": 100})).await;
    client.packet(&[("speed_hatch", json!([5001]))]).await;
    let sped = app.save(id).await.unwrap();
    assert!(sped["map"]["eggs"][0]["ts"].as_i64().unwrap() < now() - 30_000);
    assert!(sped["playerInfo"]["cash"].as_i64().unwrap() < 100);
}
