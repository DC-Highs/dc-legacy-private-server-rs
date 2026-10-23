use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::{check_range, Body};
use crate::auth::Login;
use crate::enums::Resource;
use crate::error::{not_found, ApiError, ApiResult};
use crate::game::market::{
    ensure_private_state, grant_dragon_to_player, load_or_create_market, OFFER_DRAGON, OFFER_FOOD, OFFER_GEMS, OFFER_GOLD,
};
use crate::json::{add_to, ensure_object, get as json_get, int_or};
use crate::state::AppState;
use crate::util::now_ts;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RedeemBody {
    offer_id: i64,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/market_data", get(market_data)).route("/market/redeem", post(market_redeem))
}

async fn market_data(State(state): State<AppState>, Login(user): Login) -> ApiResult<Json<Value>> {
    let (player, market) = {
        let _guard = state.storage.lock(user.id).await;
        let Some(mut loaded) = state.storage.load_player(user.id).await? else {
            return Err(not_found("Player not found"));
        };
        ensure_private_state(&mut loaded.data);
        state.storage.save_player(user.id, &loaded.data).await?;
        (loaded.data, load_or_create_market(&state, user.id).await?)
    };

    let remaining = (int_or(json_get(&market, "next_refresh_at"), 0) - now_ts()).max(0);
    Ok(Json(json!({
        "player_name": player.pointer("/playerInfo/name").and_then(Value::as_str).unwrap_or(""),
        "achievement_points": player.pointer("/privateState/achievement_points").cloned().unwrap_or(Value::from(0)),
        "next_refresh_seconds": remaining,
        "offers": market.get("offers").cloned().unwrap_or_else(|| json!([])),
    })))
}

async fn market_redeem(State(state): State<AppState>, Login(user): Login, Body(body): Body<RedeemBody>) -> ApiResult<Json<Value>> {
    check_range("offer_id", body.offer_id, 1, 1000)?;

    let _guard = state.storage.lock(user.id).await;
    let Some(mut loaded) = state.storage.load_player(user.id).await? else {
        return Err(not_found("Player not found"));
    };
    ensure_private_state(&mut loaded.data);

    let mut market = load_or_create_market(&state, user.id).await?;
    let offer_index = market
        .get("offers")
        .and_then(Value::as_array)
        .and_then(|offers| offers.iter().position(|o| int_or(json_get(o, "id"), -1) == body.offer_id))
        .ok_or_else(|| not_found("Offer not found"))?;
    let offer = market["offers"][offer_index].clone();

    if crate::json::truthy(json_get(&offer, "sold")) {
        return Err(ApiError::new(400, "already_redeemed", "This offer was already redeemed"));
    }
    let cost = int_or(json_get(&offer, "cost"), 0);
    let points = int_or(loaded.data.pointer("/privateState/achievement_points").unwrap_or(&Value::Null), 0);
    if points < cost {
        return Err(ApiError::new(400, "not_enough_points", "Not enough points"));
    }

    let root = loaded.data.as_object_mut().ok_or_else(crate::error::internal)?;
    ensure_object(root, "privateState").insert("achievement_points".into(), Value::from(points - cost));

    let kind = json_get(&offer, "type").as_str().unwrap_or("").to_string();
    let amount = int_or(json_get(&offer, "amount"), 0);
    let message = match kind.as_str() {
        OFFER_GOLD | OFFER_FOOD | OFFER_GEMS => {
            let (resource, label) = match kind.as_str() {
                OFFER_GOLD => (Resource::Gold, "gold"),
                OFFER_FOOD => (Resource::Food, "food"),
                _ => (Resource::Cash, "gems"),
            };
            add_to(ensure_object(root, "playerInfo"), resource.key(), amount);
            format!("+{amount} {label}")
        }
        OFFER_DRAGON => {
            grant_dragon_to_player(&mut loaded.data, &offer);
            format!("New dragon: {}!", json_get(&offer, "name").as_str().unwrap_or(""))
        }
        _ => String::new(),
    };

    if let Some(sold) = market.get_mut("offers").and_then(|o| o.get_mut(offer_index)).and_then(Value::as_object_mut) {
        sold.insert("sold".into(), Value::Bool(true));
    }
    state.storage.save_market(user.id, &market).await?;
    state.storage.save_player(user.id, &loaded.data).await?;
    tracing::info!("Market: player {} redeemed offer {} ({kind}) for {cost} points", user.id, body.offer_id);

    Ok(Json(json!({ "status": "ok", "message": message, "new_points": points - cost })))
}
