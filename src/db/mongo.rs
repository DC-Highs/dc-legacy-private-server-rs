use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, DateTime as BsonDateTime, Document};
use mongodb::options::{ClientOptions, Credential};
use mongodb::{Client, Collection, Database, IndexModel};
use serde_json::Value;

use super::{PendingAssist, PlayerRecord, PlayerStore};
use crate::config::Settings;
use crate::error::StoreResult;

const PLAYERS: &str = "players";
const PVP_ISLE: &str = "pvp_isle";
const PVP_PLAYERS: &str = "pvp_players";
const PENDING_ASSISTS: &str = "pending_assists";
const MAX_TAKEN_ASSISTS: i64 = 100;

pub struct MongoPlayers {
    db: Database,
}

pub async fn connect(settings: &Settings, wait_seconds: u64) -> StoreResult<MongoPlayers> {
    let deadline = std::time::Instant::now() + Duration::from_secs(wait_seconds);
    loop {
        match try_connect(settings).await {
            Ok(store) => return Ok(store),
            Err(error) if std::time::Instant::now() < deadline => {
                tracing::info!("Waiting for MongoDB... ({error})");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn try_connect(settings: &Settings) -> StoreResult<MongoPlayers> {
    let mut options = ClientOptions::parse(&settings.mongo_url).await?;
    options.server_selection_timeout = Some(Duration::from_secs(5));
    if !settings.mongo_user.is_empty() {
        options.credential = Some(
            Credential::builder()
                .username(settings.mongo_user.clone())
                .password(settings.mongo_password.clone())
                .source("admin".to_string())
                .build(),
        );
    }
    let client = Client::with_options(options)?;
    let db = client.database(&settings.mongo_db);
    db.run_command(doc! { "ping": 1 }).await?;
    Ok(MongoPlayers { db })
}

fn to_bson(value: &Value) -> StoreResult<Bson> {
    Ok(mongodb::bson::to_bson(value)?)
}

fn to_json(bson: Bson) -> StoreResult<Value> {
    Ok(mongodb::bson::from_bson(bson)?)
}

fn non_null(bson: Option<&Bson>) -> Option<&Bson> {
    bson.filter(|b| !matches!(b, Bson::Null))
}

impl MongoPlayers {
    fn players(&self) -> Collection<Document> {
        self.db.collection(PLAYERS)
    }

    fn pending(&self) -> Collection<Document> {
        self.db.collection(PENDING_ASSISTS)
    }

    fn isle(&self) -> Collection<Document> {
        self.db.collection(PVP_ISLE)
    }

    fn pvp_players(&self) -> Collection<Document> {
        self.db.collection(PVP_PLAYERS)
    }
}

#[async_trait]
impl PlayerStore for MongoPlayers {
    async fn ping(&self) -> StoreResult<()> {
        self.db.run_command(doc! { "ping": 1 }).await?;
        Ok(())
    }

    async fn ensure_indexes(&self) -> StoreResult<()> {
        self.isle().create_index(IndexModel::builder().keys(doc! { "order": 1 }).build()).await?;
        self.pending().create_index(IndexModel::builder().keys(doc! { "target": 1 }).build()).await?;
        tracing::info!("MongoDB indexes ensured");
        Ok(())
    }

    async fn get_record(&self, id: i64) -> StoreResult<Option<PlayerRecord>> {
        let found = self
            .players()
            .find_one(doc! { "_id": id })
            .projection(doc! { "data": 1, "kompu_offers": 1 })
            .await?;
        let Some(document) = found else { return Ok(None) };
        let Some(data) = document.get("data").cloned() else { return Ok(None) };
        let kompu_offers = match non_null(document.get("kompu_offers")) {
            Some(offers) => Some(to_json(offers.clone())?),
            None => None,
        };
        Ok(Some(PlayerRecord { data: to_json(data)?, kompu_offers }))
    }

    async fn exists(&self, id: i64) -> StoreResult<bool> {
        Ok(self.players().count_documents(doc! { "_id": id }).limit(1).await? > 0)
    }

    async fn get_kompu_offers(&self, id: i64) -> StoreResult<Option<Value>> {
        let found = self.players().find_one(doc! { "_id": id }).projection(doc! { "kompu_offers": 1 }).await?;
        match found.as_ref().and_then(|d| non_null(d.get("kompu_offers"))) {
            Some(offers) => Ok(Some(to_json(offers.clone())?)),
            None => Ok(None),
        }
    }

    async fn create(&self, id: i64, data: &Value, replace: bool) -> StoreResult<()> {
        let data = to_bson(data)?;
        let now = BsonDateTime::now();
        let update = if replace {
            doc! {
                "$set": { "data": data, "kompu_offers": Bson::Null, "market": Bson::Null, "updated_at": now },
                "$setOnInsert": { "created_at": now },
            }
        } else {
            doc! {
                "$setOnInsert": {
                    "data": data, "kompu_offers": Bson::Null, "market": Bson::Null,
                    "created_at": now, "updated_at": now,
                },
            }
        };
        self.players().update_one(doc! { "_id": id }, update).upsert(true).await?;
        Ok(())
    }

    async fn delete(&self, id: i64) -> StoreResult<()> {
        self.players().delete_one(doc! { "_id": id }).await?;
        self.pending().delete_many(doc! { "target": id }).await?;
        Ok(())
    }

    async fn save_data(&self, id: i64, data: &Value) -> StoreResult<()> {
        self.players()
            .update_one(doc! { "_id": id }, doc! { "$set": { "data": to_bson(data)?, "updated_at": BsonDateTime::now() } })
            .await?;
        Ok(())
    }

    async fn set_kompu_offers(&self, id: i64, offers: &Value) -> StoreResult<()> {
        self.players()
            .update_one(doc! { "_id": id }, doc! { "$set": { "kompu_offers": to_bson(offers)? } })
            .await?;
        Ok(())
    }

    async fn pending_targets(&self) -> StoreResult<Vec<i64>> {
        let targets = self.pending().distinct("target", doc! {}).await?;
        Ok(targets
            .into_iter()
            .filter_map(|b| match b {
                Bson::Int32(n) => Some(n as i64),
                Bson::Int64(n) => Some(n),
                _ => None,
            })
            .collect())
    }

    async fn take_pending_assists(&self, target: i64) -> StoreResult<Vec<PendingAssist>> {
        let documents: Vec<Document> = self
            .pending()
            .find(doc! { "target": target })
            .limit(MAX_TAKEN_ASSISTS)
            .await?
            .try_collect()
            .await?;
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let mut assists = Vec::with_capacity(documents.len());
        let mut ids = Vec::with_capacity(documents.len());
        for document in &documents {
            if let Some(id) = document.get("_id") {
                ids.push(id.clone());
            }
            let helper = match document.get("helper") {
                Some(Bson::Int32(n)) => *n as i64,
                Some(Bson::Int64(n)) => *n,
                _ => continue,
            };
            let clicks = match document.get("clicks").cloned().map(to_json).transpose()? {
                Some(Value::Array(items)) => items,
                _ => Vec::new(),
            };
            assists.push(PendingAssist { helper, clicks });
        }
        self.pending().delete_many(doc! { "_id": { "$in": ids } }).await?;
        Ok(assists)
    }

    async fn add_pending_assist(&self, target: i64, helper: i64, clicks: &[Value]) -> StoreResult<()> {
        let clicks = to_bson(&Value::Array(clicks.to_vec()))?;
        self.pending()
            .insert_one(doc! { "target": target, "helper": helper, "clicks": clicks, "at": BsonDateTime::now() })
            .await?;
        Ok(())
    }

    async fn get_market(&self, id: i64) -> StoreResult<Option<Value>> {
        let found = self.players().find_one(doc! { "_id": id }).projection(doc! { "market": 1 }).await?;
        match found.as_ref().and_then(|d| non_null(d.get("market"))) {
            Some(market) => Ok(Some(to_json(market.clone())?)),
            None => Ok(None),
        }
    }

    async fn save_market(&self, id: i64, market: &Value) -> StoreResult<()> {
        self.players().update_one(doc! { "_id": id }, doc! { "$set": { "market": to_bson(market)? } }).await?;
        Ok(())
    }

    async fn get_minigame_ts(&self, id: i64) -> StoreResult<i64> {
        let found = self.players().find_one(doc! { "_id": id }).projection(doc! { "minigame_ts": 1 }).await?;
        Ok(match found.as_ref().and_then(|d| d.get("minigame_ts")) {
            Some(Bson::Int32(n)) => *n as i64,
            Some(Bson::Int64(n)) => *n,
            Some(Bson::Double(n)) => *n as i64,
            _ => 0,
        })
    }

    async fn set_minigame_ts(&self, id: i64, timestamp: i64) -> StoreResult<()> {
        self.players().update_one(doc! { "_id": id }, doc! { "$set": { "minigame_ts": timestamp } }).await?;
        Ok(())
    }

    async fn summaries(&self, ids: &[i64]) -> StoreResult<HashMap<i64, Value>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let documents: Vec<Document> = self
            .players()
            .find(doc! { "_id": { "$in": ids.to_vec() } })
            .projection(doc! { "data.playerInfo": 1, "data.map.dragons": 1 })
            .await?
            .try_collect()
            .await?;
        let mut found = HashMap::new();
        for document in documents {
            let id = match document.get("_id") {
                Some(Bson::Int32(n)) => *n as i64,
                Some(Bson::Int64(n)) => *n,
                _ => continue,
            };
            if let Some(data) = document.get("data") {
                found.insert(id, to_json(data.clone())?);
            }
        }
        Ok(found)
    }

    async fn pvp_is_seeded(&self) -> StoreResult<bool> {
        Ok(self.isle().count_documents(doc! {}).limit(1).await? > 0)
    }

    async fn pvp_seed(&self, isle: Vec<Value>, players: Vec<(String, Value)>) -> StoreResult<()> {
        if !isle.is_empty() {
            let mut documents = Vec::with_capacity(isle.len());
            for (order, entry) in isle.iter().enumerate() {
                let id = crate::json::id_text(entry.get("id").unwrap_or(&Value::Null));
                documents.push(doc! { "_id": id, "order": order as i64, "entry": to_bson(entry)? });
            }
            self.isle().insert_many(documents).await?;
        }
        if !players.is_empty() {
            let mut documents = Vec::with_capacity(players.len());
            for (id, data) in &players {
                documents.push(doc! { "_id": id.clone(), "data": to_bson(data)? });
            }
            self.pvp_players().insert_many(documents).await?;
        }
        Ok(())
    }

    async fn pvp_page(&self, skip: i64, limit: i64) -> StoreResult<Vec<Value>> {
        let documents: Vec<Document> = self
            .isle()
            .find(doc! {})
            .sort(doc! { "order": 1 })
            .skip(skip.max(0) as u64)
            .limit(limit)
            .await?
            .try_collect()
            .await?;
        let mut entries = Vec::with_capacity(documents.len());
        for document in documents {
            if let Some(entry) = document.get("entry") {
                entries.push(to_json(entry.clone())?);
            }
        }
        Ok(entries)
    }

    async fn pvp_player(&self, id: &str) -> StoreResult<Option<Value>> {
        let found = self.pvp_players().find_one(doc! { "_id": id }).await?;
        match found.as_ref().and_then(|d| d.get("data")) {
            Some(data) => Ok(Some(to_json(data.clone())?)),
            None => Ok(None),
        }
    }

    async fn pvp_entries(&self) -> StoreResult<Vec<Value>> {
        let documents: Vec<Document> = self
            .isle()
            .find(doc! {})
            .projection(doc! { "entry.id": 1, "entry.islandNumber": 1, "order": 1 })
            .await?
            .try_collect()
            .await?;
        let mut entries = Vec::with_capacity(documents.len());
        for document in documents {
            if let Some(entry) = document.get("entry") {
                entries.push(to_json(entry.clone())?);
            }
        }
        Ok(entries)
    }

    async fn pvp_count(&self) -> StoreResult<i64> {
        Ok(self.isle().count_documents(doc! {}).await? as i64)
    }

    async fn pvp_insert(&self, id: &str, order: i64, entry: Value, data: Value) -> StoreResult<()> {
        self.isle().insert_one(doc! { "_id": id, "order": order, "entry": to_bson(&entry)? }).await?;
        self.pvp_players().insert_one(doc! { "_id": id, "data": to_bson(&data)? }).await?;
        Ok(())
    }
}
