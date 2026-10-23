use std::collections::HashSet;

use rand::seq::IndexedRandom;
use rand::Rng;
use serde_json::{json, Map, Value};

use crate::enums::group;
use crate::error::{internal, ApiResult};
use crate::json::{ensure_array, ensure_object, get, int_or};
use crate::state::AppState;
use crate::util::now_ts;

pub const MARKET_REFRESH_SECONDS: i64 = 6 * 60 * 60;

pub const OFFER_GOLD: &str = "gold";
pub const OFFER_FOOD: &str = "food";
pub const OFFER_GEMS: &str = "gems";
pub const OFFER_DRAGON: &str = "dragon";

struct ResourceRule {
    min: i64,
    max: i64,
    per_unit: f64,
    min_cost: i64,
    max_cost: i64,
    step: i64,
}

fn rule(kind: &str) -> ResourceRule {
    match kind {
        OFFER_GOLD => ResourceRule { min: 500_000, max: 40_000_000, per_unit: 4000.0, min_cost: 125, max_cost: 10_000, step: 50_000 },
        OFFER_FOOD => ResourceRule { min: 350_000, max: 10_000_000, per_unit: 1000.0, min_cost: 350, max_cost: 10_000, step: 10_000 },
        _ => ResourceRule { min: 100, max: 2500, per_unit: 0.4, min_cost: 250, max_cost: 6250, step: 1 },
    }
}

fn dragon_base_cost(category: i64) -> i64 {
    match category {
        3 => 4000,
        4 => 7500,
        5 => 15_000,
        other => 15_000 + (other - 5).max(0) * 5000,
    }
}

fn attribute_surcharge(code: &str) -> i64 {
    match code {
        "d" | "li" | "i" => 500,
        "wr" => 1000,
        "pu" => 2000,
        "l" => 4000,
        _ => 0,
    }
}

pub fn ensure_private_state(data: &mut Value) {
    if !data.is_object() {
        *data = Value::Object(Map::new());
    }
    if let Some(root) = data.as_object_mut() {
        let private_state = ensure_object(root, "privateState");
        private_state.entry("achievement_points").or_insert_with(|| Value::from(0));
    }
}

fn resource_cost(kind: &str, amount: i64) -> i64 {
    let rule = rule(kind);
    let cost = if kind == OFFER_GEMS {
        (amount as f64 * rule.per_unit).round() as i64
    } else {
        (amount as f64 / rule.per_unit).floor() as i64
    };
    cost.min(rule.max_cost).max(rule.min_cost)
}

fn roll_resource_offer(kind: &str) -> Value {
    let rule = rule(kind);
    let steps = (rule.max - rule.min) / rule.step;
    let amount = rule.min + rule.step * rand::rng().random_range(0..=steps);
    json!({ "type": kind, "amount": amount, "cost": resource_cost(kind, amount) })
}

fn dragon_cost(item: &Value) -> i64 {
    let category = int_or(get(item, "category"), 3);
    let surcharge: i64 = get(item, "attributes")
        .as_array()
        .map(|attrs| attrs.iter().filter_map(Value::as_str).map(attribute_surcharge).sum())
        .unwrap_or(0);
    dragon_base_cost(category) + surcharge
}

fn roll_dragon_offer(pool: &[&Value], already_chosen: &HashSet<i64>) -> Option<Value> {
    let fresh: Vec<&&Value> = pool.iter().filter(|d| !already_chosen.contains(&int_or(get(d, "id"), -1))).collect();
    let mut rng = rand::rng();
    let item: &Value = if fresh.is_empty() { pool.choose(&mut rng)? } else { **fresh.choose(&mut rng)? };
    Some(json!({
        "type": OFFER_DRAGON,
        "dragon_id": get(item, "id"),
        "name": get(item, "name"),
        "img": get(item, "img_name"),
        "category": get(item, "category"),
        "attributes": item.get("attributes").cloned().unwrap_or_else(|| json!([])),
        "cost": dragon_cost(item),
    }))
}

pub async fn generate_market_offers(state: &AppState, user_id: i64) -> ApiResult<Value> {
    let config = &state.storage.game.config;
    let pool: Vec<&Value> = config
        .items()
        .iter()
        .filter(|item| get(item, "group_type").as_str() == Some(group::DRAGON) && int_or(get(item, "category"), 0) >= 3)
        .collect();

    let mut chosen = HashSet::new();
    let mut dragon_offers = Vec::new();
    for _ in 0..2 {
        let offer = roll_dragon_offer(&pool, &chosen).ok_or_else(internal)?;
        chosen.insert(int_or(get(&offer, "dragon_id"), -1));
        dragon_offers.push(offer);
    }

    let mut offers = vec![
        dragon_offers[0].clone(),
        roll_resource_offer(OFFER_GOLD),
        roll_resource_offer(OFFER_FOOD),
        roll_resource_offer(OFFER_GEMS),
        dragon_offers[1].clone(),
        roll_resource_offer(OFFER_GOLD),
        roll_resource_offer(OFFER_FOOD),
        roll_resource_offer(OFFER_GEMS),
    ];
    for (index, offer) in offers.iter_mut().enumerate() {
        if let Some(object) = offer.as_object_mut() {
            object.insert("id".into(), Value::from(index as i64 + 1));
            object.insert("sold".into(), Value::Bool(false));
        }
    }

    let now = now_ts();
    let market = json!({
        "user_id": user_id.to_string(),
        "generated_at": now,
        "next_refresh_at": now + MARKET_REFRESH_SECONDS,
        "offers": offers,
    });
    state.storage.save_market(user_id, &market).await?;
    tracing::info!("Generated {} market offers for player {user_id}", 8);
    Ok(market)
}

pub async fn load_or_create_market(state: &AppState, user_id: i64) -> ApiResult<Value> {
    match state.storage.get_market(user_id).await? {
        Some(market) if crate::json::truthy(&market) && now_ts() < int_or(get(&market, "next_refresh_at"), 0) => Ok(market),
        _ => generate_market_offers(state, user_id).await,
    }
}

pub fn grant_dragon_to_player(data: &mut Value, offer: &Value) {
    let Some(dragon_iid) = offer.get("dragon_id").filter(|id| crate::json::truthy(id)) else {
        tracing::error!("The offer does not contain a valid dragon id");
        return;
    };
    let Some(root) = data.as_object_mut() else { return };
    let game_map = ensure_object(root, "map");
    let store = ensure_object(game_map, "store");
    ensure_array(store, "dragons").push(json!({ "iid": dragon_iid, "lvl": 0 }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_costs_are_clamped() {
        assert_eq!(resource_cost(OFFER_GOLD, 500_000), 125);
        assert_eq!(resource_cost(OFFER_GOLD, 40_000_000), 10_000);
        assert_eq!(resource_cost(OFFER_GEMS, 100), 250);
        assert_eq!(resource_cost(OFFER_GEMS, 2500), 1000);
        assert_eq!(resource_cost(OFFER_FOOD, 350_000), 350);
    }

    #[test]
    fn rolled_resources_stay_in_range_and_on_the_step() {
        for _ in 0..200 {
            let offer = roll_resource_offer(OFFER_GOLD);
            let amount = offer["amount"].as_i64().unwrap();
            assert!((500_000..=40_000_000).contains(&amount) && (amount - 500_000) % 50_000 == 0);
        }
    }

    #[test]
    fn dragon_costs_follow_category_and_element() {
        assert_eq!(dragon_cost(&json!({"category": 3, "attributes": ["f"]})), 4000);
        assert_eq!(dragon_cost(&json!({"category": 5, "attributes": ["l", "d"]})), 15_000 + 4000 + 500);
        assert_eq!(dragon_cost(&json!({"category": 7, "attributes": []})), 15_000 + 10_000);
    }
}
