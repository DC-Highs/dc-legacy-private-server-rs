use rand::seq::{IndexedRandom, SliceRandom};
use rand::Rng;
use serde_json::{json, Map, Value};

use crate::enums::group;
use crate::gamedata::PlayerConfig;
use crate::json::{ensure_object, get, int_or, truthy};

pub const FALLBACK_COMMON_DRAGON_ID: i64 = 1011;
pub const FALLBACK_RARE_DRAGON_ID: i64 = 1041;

pub const OFFER_DURATION_SECONDS: i64 = 259_200;
pub const MIN_COOLDOWN_SECONDS: i64 = 259_200;
pub const MAX_COOLDOWN_SECONDS: i64 = 604_800;
const UNIQUE_DRAGON_CHANCE_PERCENT: i64 = 10;
const UNIQUE_DRAGON_CHANCE: f64 = 0.10;

fn categorized_dragons(config: &PlayerConfig) -> Vec<(i64, i64)> {
    config
        .items()
        .iter()
        .filter(|item| get(item, "group_type").as_str() == Some(group::DRAGON))
        .filter_map(|item| {
            let category = get(item, "category").as_i64()?;
            let id = get(item, "id").as_i64()?;
            Some((id, category))
        })
        .collect()
}

fn ids_in(dragons: &[(i64, i64)], categories: &[i64]) -> Vec<i64> {
    dragons.iter().filter(|(_, c)| categories.contains(c)).map(|(id, _)| *id).collect()
}

fn sample(pool: &[i64], count: usize) -> Vec<i64> {
    pool.choose_multiple(&mut rand::rng(), count).copied().collect()
}

pub fn generate_black_market_boxes(config: &PlayerConfig) -> (i64, Vec<i64>, Vec<i64>) {
    let dragons = categorized_dragons(config);
    let cat_2_3 = ids_in(&dragons, &[2, 3]);
    let cat_3_4_5 = ids_in(&dragons, &[3, 4, 5]);
    let cat_4 = ids_in(&dragons, &[4]);
    let cat_5 = ids_in(&dragons, &[5]);
    let mut rng = rand::rng();

    let unique_dragon = cat_3_4_5.choose(&mut rng).copied().unwrap_or(FALLBACK_RARE_DRAGON_ID);

    let mut boxes: Vec<i64> = Vec::with_capacity(6);
    if cat_2_3.len() >= 3 {
        boxes.extend(sample(&cat_2_3, 3));
    } else {
        boxes.extend(&cat_2_3);
        while boxes.len() < 3 {
            boxes.push(FALLBACK_COMMON_DRAGON_ID);
        }
    }

    if cat_4.len() >= 2 {
        boxes.extend(sample(&cat_4, 2));
    } else {
        boxes.extend(&cat_4);
        while boxes.len() < 5 {
            boxes.push(FALLBACK_COMMON_DRAGON_ID);
        }
    }

    if rng.random_range(1..=100) <= UNIQUE_DRAGON_CHANCE_PERCENT {
        boxes.push(unique_dragon);
    } else {
        boxes.push(cat_5.choose(&mut rng).copied().unwrap_or(FALLBACK_RARE_DRAGON_ID));
    }

    let mut shuffled = boxes.clone();
    shuffled.shuffle(&mut rng);
    (unique_dragon, boxes, shuffled)
}

pub fn build_kompu_offers_payload(config: &PlayerConfig, start_timestamp: i64, unique_dragon: i64) -> Value {
    let dragons = categorized_dragons(config);
    let cat_1 = ids_in(&dragons, &[1]);
    let cat_2_3 = ids_in(&dragons, &[2, 3]);
    let cat_4_5 = ids_in(&dragons, &[4, 5]);
    let cat_3_4_5 = ids_in(&dragons, &[3, 4, 5]);

    let pool = |source: &[i64], count: usize, fallback: i64| -> Vec<i64> {
        if source.is_empty() {
            vec![fallback]
        } else {
            sample(source, count.min(source.len()))
        }
    };

    json!([{
        "id": 1,
        "starts_at": start_timestamp,
        "duration": OFFER_DURATION_SECONDS,
        "game_type": 4,
        "resetable": 0,
        "pick_prize": 0,
        "hurry_up_cost": 10,
        "timer_between": 21600,
        "viral_icon": [],
        "rewards": {
            "uniqueDragonId": unique_dragon,
            "prizes": [
                pool(&cat_1, 10, FALLBACK_COMMON_DRAGON_ID),
                pool(&cat_2_3, 10, FALLBACK_COMMON_DRAGON_ID),
                pool(&cat_4_5, 7, FALLBACK_RARE_DRAGON_ID),
                pool(&cat_3_4_5, 5, FALLBACK_RARE_DRAGON_ID),
            ],
        },
    }])
}

fn store_boxes(private_state: &mut Map<String, Value>, unique_dragon: i64, boxes: Vec<i64>, shuffled: Vec<i64>) {
    private_state.insert("BlackMarketUniqueDragon".into(), Value::from(unique_dragon));
    private_state.insert("kompuBoxesDragons".into(), json!(boxes));
    private_state.insert("kompuBoxesShuffledDragons".into(), json!(shuffled));
}

pub fn schedule_black_market(private_state: &mut Map<String, Value>, config: &PlayerConfig, current_ts: i64, delay: i64) {
    private_state.insert("BlackMarketTs".into(), Value::from(current_ts));
    private_state.insert("BlackMarketStart".into(), Value::from(current_ts + delay));
    private_state.insert("BlackMarketEnd".into(), Value::from(current_ts + delay + OFFER_DURATION_SECONDS));

    let (unique_dragon, boxes, shuffled) = generate_black_market_boxes(config);
    store_boxes(private_state, unique_dragon, boxes, shuffled);
}

pub fn roll_kompu_pool(config: &PlayerConfig) -> Option<(Vec<i64>, bool)> {
    let unique_dragon_id = config
        .kompu_offers()
        .as_array()
        .and_then(|offers| offers.first())
        .map(|offer| get(get(offer, "rewards"), "uniqueDragonId"))
        .filter(|id| truthy(id))
        .and_then(Value::as_i64);

    let mut cat_2_3_4 = Vec::new();
    let mut cat_4 = Vec::new();
    let mut cat_5 = Vec::new();
    for item in config.items() {
        if get(item, "group_type").as_str() != Some(group::DRAGON) {
            continue;
        }
        let category = int_or(get(item, "category"), 1);
        let Some(id) = get(item, "id").as_i64() else { continue };
        if [2, 3, 4].contains(&category) {
            cat_2_3_4.push(id);
        }
        if category == 4 {
            cat_4.push(id);
        }
        if category == 5 {
            cat_5.push(id);
        }
    }

    let mut rng = rand::rng();
    let mut pool = sample(&cat_2_3_4, 4.min(cat_2_3_4.len()));
    while pool.len() < 4 {
        pool.push(*cat_2_3_4.choose(&mut rng)?);
    }
    pool.push(*cat_4.choose(&mut rng)?);

    match unique_dragon_id {
        Some(unique) if rng.random::<f64>() <= UNIQUE_DRAGON_CHANCE => {
            pool.push(unique);
            Some((pool, true))
        }
        _ => {
            pool.push(*cat_5.choose(&mut rng)?);
            Some((pool, false))
        }
    }
}

pub fn check_and_update_black_market(data: &mut Value, config: &PlayerConfig, user_id: i64) -> Option<Value> {
    let current_ts = int_or(get(data, "timestamp"), 0);
    let start = int_or(get(get(data, "privateState"), "BlackMarketStart"), 0);
    let end = int_or(get(get(data, "privateState"), "BlackMarketEnd"), 0);
    if start == 0 || end == 0 {
        return None;
    }

    if current_ts >= end {
        let cooldown = rand::rng().random_range(MIN_COOLDOWN_SECONDS..=MAX_COOLDOWN_SECONDS);
        let (unique_dragon, boxes, shuffled) = generate_black_market_boxes(config);
        let private_state = ensure_object(data.as_object_mut()?, "privateState");
        private_state.insert("BlackMarketTs".into(), Value::from(current_ts));
        private_state.insert("BlackMarketStart".into(), Value::from(current_ts + cooldown));
        private_state.insert("BlackMarketEnd".into(), Value::from(current_ts + cooldown + OFFER_DURATION_SECONDS));
        tracing::info!(
            "Black market expired for {user_id}, next one in {} days, new boxes waiting: {boxes:?}",
            cooldown / 86_400
        );
        store_boxes(private_state, unique_dragon, boxes, shuffled);
        return Some(json!([]));
    }

    if start <= current_ts && current_ts < end && !truthy(config.kompu_offers()) {
        let unique_dragon = int_or(get(data, "BlackMarketUniqueDragon"), FALLBACK_RARE_DRAGON_ID);
        tracing::info!("Black market is now visible in the HUD of player {user_id}");
        return Some(build_kompu_offers_payload(config, start, unique_dragon));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::GameData;

    fn config() -> PlayerConfig {
        let items: Vec<Value> = (1..=40)
            .map(|n| json!({"id": 1000 + n, "group_type": "DRAGON", "category": (n % 5) + 1}))
            .collect();
        GameData::from_values(json!({"items": items}), json!({})).player_config(None)
    }

    #[test]
    fn a_window_has_six_boxes_and_a_unique_dragon() {
        let (unique, boxes, shuffled) = generate_black_market_boxes(&config());
        assert_eq!(boxes.len(), 6);
        let (mut a, mut b) = (boxes.clone(), shuffled);
        a.sort();
        b.sort();
        assert_eq!(a, b);
        assert!(unique > 1000);
    }

    #[test]
    fn expired_windows_are_rescheduled_and_visible_ones_get_offers() {
        let cfg = config();
        let mut data = json!({"timestamp": 500, "privateState": {"BlackMarketStart": 100, "BlackMarketEnd": 400}});
        assert_eq!(check_and_update_black_market(&mut data, &cfg, 1), Some(json!([])));
        assert!(data["privateState"]["BlackMarketStart"].as_i64().unwrap() > 500);

        let mut open = json!({"timestamp": 200, "privateState": {"BlackMarketStart": 100, "BlackMarketEnd": 400}});
        let offers = check_and_update_black_market(&mut open, &cfg, 1).unwrap();
        assert_eq!(offers[0]["starts_at"], 100);
        assert_eq!(offers[0]["rewards"]["prizes"].as_array().unwrap().len(), 4);

        let mut idle = json!({"timestamp": 200, "privateState": {}});
        assert_eq!(check_and_update_black_market(&mut idle, &cfg, 1), None);
    }
}
