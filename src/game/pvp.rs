//! PvP opponents (stored next to the saves) and their generation.
use std::collections::HashMap;

use rand::seq::IndexedRandom;
use rand::Rng;
use serde_json::{json, Value};

use crate::config::PVP_PAGE_SIZE;
use crate::enums::group;
use crate::error::ApiResult;
use crate::json::{get, id_text, int_or};
use crate::names::{DRAGON_NAMES, FIRST_NAMES, LAST_NAMES};
use crate::state::AppState;

pub const PVP_ISLE_FILE: &str = "get_pvp_isle.json";
pub const PVP_PLAYERS_FILE: &str = "pvp_players_info.json";
const OPPONENTS_PER_BATCH: i64 = 8;
const OPPONENT_PICTURE: &str = "http://graph.facebook.com/1000000000000/picture?type=square";

pub async fn seed_pvp_data(state: &AppState) -> ApiResult<()> {
    let store = state.storage.store();
    if store.pvp_is_seeded().await? {
        return Ok(());
    }

    let seed_dir = &state.settings.seed_dir;
    let isle_path = seed_dir.join(PVP_ISLE_FILE);
    let players_path = seed_dir.join(PVP_PLAYERS_FILE);
    if !isle_path.exists() || !players_path.exists() {
        tracing::warn!("PvP seed files not found in {}, the PvP isle will be empty", seed_dir.display());
        return Ok(());
    }

    let read = |path: &std::path::Path| -> Option<Value> {
        std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok())
    };
    let (Some(isle), Some(players)) = (read(&isle_path), read(&players_path)) else {
        tracing::warn!("PvP seed files in {} are not valid JSON, the PvP isle will be empty", seed_dir.display());
        return Ok(());
    };

    let isle = isle.as_array().cloned().unwrap_or_default();
    let players: Vec<(String, Value)> =
        players.as_object().map(|o| o.iter().map(|(id, data)| (id.clone(), data.clone())).collect()).unwrap_or_default();
    let total = isle.len();
    store.pvp_seed(isle, players).await?;
    tracing::info!("Seeded {total} PvP opponents");
    Ok(())
}

pub async fn pvp_page(state: &AppState, level: i64) -> ApiResult<Vec<Value>> {
    let skip = ((level - 1) * PVP_PAGE_SIZE).max(0);
    Ok(state.storage.store().pvp_page(skip, PVP_PAGE_SIZE).await?)
}

pub async fn get_pvp_player(state: &AppState, opponent_id: &str) -> ApiResult<Option<Value>> {
    Ok(state.storage.store().pvp_player(opponent_id).await?)
}

fn build_dragon_pool(items: &[Value]) -> HashMap<i64, Vec<i64>> {
    let mut pool: HashMap<i64, Vec<i64>> = (1..=5).map(|category| (category, Vec::new())).collect();
    for item in items {
        if get(item, "group_type").as_str() != Some(group::DRAGON) {
            continue;
        }
        let category = int_or(get(item, "category"), 1);
        if let (Some(id), Some(list)) = (get(item, "id").as_i64(), pool.get_mut(&category)) {
            list.push(id);
        }
    }
    pool
}

fn dragon_type_for_level(level: i64, pool: &HashMap<i64, Vec<i64>>) -> Option<i64> {
    let allowed: &[i64] = if level <= 15 {
        &[1, 2]
    } else if level <= 30 {
        &[2, 3, 4]
    } else {
        &[3, 4, 5]
    };
    let mut categories: Vec<i64> = allowed.iter().copied().filter(|c| pool.get(c).map(|l| !l.is_empty()).unwrap_or(false)).collect();
    if categories.is_empty() {
        categories.push(1);
    }
    let mut rng = rand::rng();
    let category = *categories.choose(&mut rng)?;
    pool.get(&category)?.choose(&mut rng).copied()
}

fn roll_team_size() -> usize {
    let roll: f64 = rand::rng().random();
    if roll <= 0.85 {
        3
    } else if roll <= 0.95 {
        2
    } else {
        1
    }
}

pub async fn generate_pvp_opponents(state: &AppState) -> ApiResult<i64> {
    let store = state.storage.store();
    let dragon_pool = build_dragon_pool(state.storage.game.config.items());

    let entries = store.pvp_entries().await?;
    let last_id = entries.iter().filter_map(|e| id_text(get(e, "id")).parse::<i64>().ok()).max().unwrap_or(100_000_000);
    let last_island = entries.iter().map(|e| int_or(get(e, "islandNumber"), 1)).max().unwrap_or(1);
    let next_order = store.pvp_count().await?;

    for i in 1..=OPPONENTS_PER_BATCH {
        let (new_id, entry, data, summary) = {
            let mut rng = rand::rng();
            let new_id = (last_id + i).to_string();
            let name = format!(
                "{} {}",
                FIRST_NAMES.choose(&mut rng).copied().unwrap_or("Deus"),
                LAST_NAMES.choose(&mut rng).copied().unwrap_or("Vega")
            );
            let level: i64 = rng.random_range(12..=60);
            let pvp_level: i64 = rng.random_range(1..=(level / 4).max(1));
            let pvp_isle = match level {
                ..=19 => 0,
                20..=34 => 1,
                35..=49 => 2,
                _ => 3,
            };
            let entry = json!({
                "id": new_id, "name": name, "level": level, "pvplevel": pvp_level, "pvpIsle": pvp_isle,
                "islandNumber": last_island + (i % 2), "pic": OPPONENT_PICTURE,
            });

            let team_size = roll_team_size();
            let mut dragons = Vec::new();
            let mut team = Vec::new();
            for index in 1..=team_size as i64 {
                dragons.push(json!({
                    "id": index,
                    "type": dragon_type_for_level(level, &dragon_pool),
                    "xp": rng.random_range(16..=156),
                    "habitatId": 0,
                    "name": DRAGON_NAMES.choose(&mut rng).copied().unwrap_or("Roxy"),
                    "ts": 0, "work_id": [], "attack": [],
                }));
                team.push(json!({ "id": index }));
            }
            let data = json!({ "map": { "dragons": dragons }, "privateState": { "teams": { "pvp": team } } });
            (new_id, entry, data, (name, level, team_size))
        };

        store.pvp_insert(&new_id, next_order + i - 1, entry, data).await?;
        tracing::info!("Generated PvP opponent {} (id={new_id}, level={}, dragons={})", summary.0, summary.1, summary.2);
    }

    let total = store.pvp_count().await?;
    tracing::info!("PvP data updated, total opponents: {total}");
    Ok(total)
}
