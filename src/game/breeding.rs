use std::collections::{BTreeSet, HashMap};

use rand::seq::IndexedRandom;
use rand::Rng;
use serde_json::Value;

use crate::enums::{attr, group};
use crate::json::{get, int_or};

pub const FALLBACK_DRAGON_ID: i64 = 1011;

const LEGENDARY_FROM_COMBO_CHANCE: f64 = 15.0;

const PURE_DRAGON_BASE_ID: i64 = 1081;
const PURE_CHANCE_BASE: f64 = 3.0;
const PURE_CHANCE_BOOSTED: f64 = 5.0;

const PURE_ELEMENT_IDS: [i64; 8] = [1082, 1083, 1084, 1085, 1086, 1087, 1088, 1089];
const PURE_ELEMENT_CHANCE_BASE: f64 = 5.0;
const PURE_ELEMENT_CHANCE_BOOSTED: f64 = 7.5;

fn difficulty_weight(difficulty: i64, boosted: bool) -> f64 {
    let (one, two, three) = if boosted { (40.0, 35.0, 25.0) } else { (55.0, 25.0, 5.0) };
    match difficulty {
        1 => one,
        2 => two,
        3 => three,
        _ => 5.0,
    }
}

fn category_multiplier(category: i64) -> f64 {
    match category {
        1 => 0.65,
        2 => 0.90,
        3 => 0.55,
        4 => 0.22,
        5 => 0.15,
        _ => 0.01,
    }
}

fn legendary_combo(pool: &BTreeSet<&str>) -> Option<&'static [i64]> {
    let is = |attrs: &[&str]| pool.len() == attrs.len() && attrs.iter().all(|a| pool.contains(a));
    if is(&[attr::WATER, attr::PLANT, attr::ICE, attr::FIRE]) {
        Some(&[1052, 1053])
    } else if is(&[attr::EARTH, attr::METAL, attr::FIRE, attr::ICE]) {
        Some(&[1041, 1052, 1053, 1054])
    } else if is(&[attr::WATER, attr::DARK]) {
        Some(&[1052])
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct DragonInfo {
    pub id: i64,
    pub name: String,
    pub attributes: BTreeSet<String>,
    pub difficulty: i64,
    pub category: i64,
    pub breeding_time: i64,
    pub breedable: i64,
}

#[derive(Default)]
pub struct DragonDb {
    pub dragons: Vec<DragonInfo>,
    index: HashMap<i64, usize>,
}

impl DragonDb {
    pub fn from_items(items: &[Value]) -> DragonDb {
        let mut db = DragonDb::default();
        for item in items {
            if get(item, "group_type").as_str() != Some(group::DRAGON) {
                continue;
            }
            let Some(id) = get(item, "id").as_i64() else { continue };
            let info = DragonInfo {
                id,
                name: get(item, "name").as_str().unwrap_or("Unknown").to_string(),
                attributes: get(item, "attributes")
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
                difficulty: int_or(get(item, "difficulty"), 3),
                category: int_or(get(item, "category"), 1),
                breeding_time: int_or(get(item, "breeding_time"), 86_400),
                breedable: int_or(get(item, "breedable"), 0),
            };
            match db.index.get(&id) {
                Some(&position) => db.dragons[position] = info,
                None => {
                    db.index.insert(id, db.dragons.len());
                    db.dragons.push(info);
                }
            }
        }
        db
    }

    pub fn get(&self, id: i64) -> Option<&DragonInfo> {
        self.index.get(&id).map(|&position| &self.dragons[position])
    }

    pub fn contains(&self, id: i64) -> bool {
        self.index.contains_key(&id)
    }
}

pub fn get_breed_result(
    dragon1_type: Option<i64>,
    dragon2_type: Option<i64>,
    level1: i64,
    level2: i64,
    db: &DragonDb,
    boosted: bool,
) -> i64 {
    let (Some(parent1), Some(parent2)) = (dragon1_type.and_then(|t| db.get(t)), dragon2_type.and_then(|t| db.get(t)))
    else {
        tracing::warn!("Unknown parent types ({dragon1_type:?}, {dragon2_type:?}), falling back to Terra");
        return FALLBACK_DRAGON_ID;
    };

    let mut rng = rand::rng();
    let pool: BTreeSet<&str> =
        parent1.attributes.iter().chain(parent2.attributes.iter()).map(String::as_str).collect();

    let is_leg1 = parent1.attributes.contains(attr::LEGENDARY);
    let is_leg2 = parent2.attributes.contains(attr::LEGENDARY);

    if is_leg1 != is_leg2 {
        tracing::debug!("Legendary + common cross, picking a random dragon from the database");
        let valid_any: Vec<&DragonInfo> = db
            .dragons
            .iter()
            .filter(|d| d.breedable > 0 && level1 >= d.breedable && level2 >= d.breedable)
            .collect();
        if let Some(chosen) = valid_any.choose(&mut rng) {
            return chosen.id;
        }
    }

    if is_leg1 && is_leg2 {
        let pure_chance = if boosted { PURE_CHANCE_BOOSTED } else { PURE_CHANCE_BASE };
        let pure_element_chance = if boosted { PURE_ELEMENT_CHANCE_BOOSTED } else { PURE_ELEMENT_CHANCE_BASE };
        let roll = rng.random_range(0.0..=100.0);

        if roll < pure_chance {
            if db.contains(PURE_DRAGON_BASE_ID) {
                return PURE_DRAGON_BASE_ID;
            }
        } else if roll < pure_chance + pure_element_chance {
            let mut pure_with_element: Vec<i64> = db
                .dragons
                .iter()
                .filter(|d| d.attributes.contains(attr::PURE) && d.attributes.len() > 1)
                .map(|d| d.id)
                .collect();
            if pure_with_element.is_empty() {
                pure_with_element = PURE_ELEMENT_IDS.iter().copied().filter(|id| db.contains(*id)).collect();
            }
            if let Some(chosen) = pure_with_element.choose(&mut rng) {
                return *chosen;
            }
        }
    }

    if boosted {
        if let Some(combo) = legendary_combo(&pool) {
            if rng.random_range(0.0..=100.0) < LEGENDARY_FROM_COMBO_CHANCE {
                let possible: Vec<i64> = combo.iter().copied().filter(|id| db.contains(*id)).collect();
                if let Some(chosen) = possible.choose(&mut rng) {
                    return *chosen;
                }
            }
        }
    }

    let mut candidates: Vec<(i64, f64)> = Vec::new();
    for dragon in &db.dragons {
        if dragon.breedable == 0 || level1 < dragon.breedable || level2 < dragon.breedable {
            continue;
        }
        if dragon.attributes.is_empty() || !dragon.attributes.iter().all(|a| pool.contains(a.as_str())) {
            continue;
        }
        if dragon.attributes.contains(attr::LEGENDARY) || dragon.attributes.contains(attr::PURE) {
            continue;
        }
        let weight = difficulty_weight(dragon.difficulty, boosted) * category_multiplier(dragon.category);
        if weight > 0.0 {
            candidates.push((dragon.id, weight));
        }
    }

    if candidates.is_empty() {
        tracing::warn!("No valid candidates by level/attributes, falling back to the first parent");
        return parent1.id;
    }

    let total: f64 = candidates.iter().map(|(_, w)| w).sum();
    let roll = rng.random_range(0.0..=total);
    let mut cumulative = 0.0;
    for (id, weight) in &candidates {
        cumulative += weight;
        if roll <= cumulative {
            return *id;
        }
    }
    candidates[candidates.len() - 1].0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db() -> DragonDb {
        DragonDb::from_items(&[
            json!({"id": 1, "group_type": "DRAGON", "name": "Terra", "attributes": ["e"], "difficulty": 1, "category": 1, "breedable": 1}),
            json!({"id": 2, "group_type": "DRAGON", "name": "Flame", "attributes": ["f"], "difficulty": 1, "category": 1, "breedable": 1}),
            json!({"id": 3, "group_type": "DRAGON", "name": "Lava", "attributes": ["e", "f"], "difficulty": 2, "category": 2, "breedable": 5}),
            json!({"id": 9, "group_type": "FARM", "name": "Not a dragon"}),
        ])
    }

    #[test]
    fn unknown_parents_fall_back_to_terra() {
        assert_eq!(get_breed_result(Some(99), Some(1), 5, 5, &db(), false), FALLBACK_DRAGON_ID);
        assert_eq!(get_breed_result(None, Some(1), 5, 5, &db(), false), FALLBACK_DRAGON_ID);
    }

    #[test]
    fn offspring_respects_attributes_and_levels() {
        let db = db();
        for _ in 0..50 {
            let low = get_breed_result(Some(1), Some(2), 2, 2, &db, false);
            assert!([1, 2].contains(&low), "level 2 parents cannot produce the level-5 dragon");
        }
        let seen: BTreeSet<i64> = (0..400).map(|_| get_breed_result(Some(1), Some(2), 9, 9, &db, false)).collect();
        assert!(seen.contains(&3));
    }
}
