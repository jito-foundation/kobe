//! DB model for a BAM validator.

use std::{collections::HashMap, str::FromStr, time::Instant};

use chrono::{serde::ts_seconds, DateTime, Utc};
use futures::TryStreamExt;
use mongodb::{bson, bson::doc, options::FindOneOptions, Collection};
use serde::{Deserialize, Serialize};
use solana_pubkey::{ParsePubkeyError, Pubkey};

use crate::{constants::BAM_VALIDATOR_COLLECTION_NAME, db_models::error::DataStoreError};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct BamValidator {
    /// Active stake lamports
    active_stake: u64,

    /// Epoch number
    epoch: u64,

    /// Identity account public key
    identity_account: String,

    /// Timestamp
    #[serde(with = "ts_seconds")]
    timestamp: DateTime<Utc>,

    /// Vote account public key
    vote_account: String,
}

impl Default for BamValidator {
    fn default() -> Self {
        let timestamp = Utc::now();

        Self {
            active_stake: 0,
            epoch: 0,
            identity_account: String::new(),
            timestamp,
            vote_account: String::new(),
        }
    }
}

impl BamValidator {
    /// Initialize a [`BamValidator`]
    pub fn new(active_stake: u64, epoch: u64, identity_account: &str, vote_account: &str) -> Self {
        let timestamp = Utc::now();

        Self {
            active_stake,
            epoch,
            identity_account: identity_account.to_string(),
            timestamp,
            vote_account: vote_account.to_string(),
        }
    }

    /// Get active stake lamports
    pub fn get_active_stake(&self) -> u64 {
        self.active_stake
    }

    /// Set active stake number
    pub fn set_active_stake(&mut self, active_stake: u64) {
        self.active_stake = active_stake;
    }

    /// Get epoch number
    pub fn get_epoch(&self) -> u64 {
        self.epoch
    }

    /// Set epoch number
    pub fn set_epoch(&mut self, epoch: u64) {
        self.epoch = epoch;
    }

    /// Get identity account pubkey (node public key)
    pub fn get_identity_account(&self) -> Result<Pubkey, ParsePubkeyError> {
        Pubkey::from_str(&self.identity_account)
    }

    /// Set identity account public key
    pub fn set_identity_account(&mut self, identity_account: String) {
        self.identity_account = identity_account;
    }

    /// Get vote account pubkey
    pub fn get_vote_account(&self) -> Result<Pubkey, ParsePubkeyError> {
        Pubkey::from_str(&self.vote_account)
    }

    /// Set vote account public key
    pub fn set_vote_account(&mut self, vote_account: String) {
        self.vote_account = vote_account;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TotalStakeDbResult {
    pub total_stake_lamports: u64,
}

#[derive(Clone)]
pub struct BamValidatorStore {
    collection: Collection<BamValidator>,
}

impl BamValidatorStore {
    pub const COLLECTION: &'static str = BAM_VALIDATOR_COLLECTION_NAME;

    /// Initialize a [`BamValidatorStore`]
    pub fn new(collection: Collection<BamValidator>) -> Self {
        Self { collection }
    }

    /// Insert many [`BamValidator`]
    pub async fn insert_many(&self, items: &[BamValidator]) -> Result<(), DataStoreError> {
        let start = Instant::now();
        self.collection.insert_many(items, None).await?;

        log::info!(
            "done writing {:#?} items to db, took {}ms",
            items.len(),
            start.elapsed().as_millis()
        );

        Ok(())
    }

    pub async fn find(&self, epoch: u64) -> Result<Vec<BamValidator>, DataStoreError> {
        let filter = doc! {"epoch": epoch as u32};

        let cursor = self.collection.find(filter, None).await?;
        let validators: Vec<BamValidator> = cursor.try_collect().await?;

        Ok(validators)
    }

    pub async fn get_highest_epoch(&self) -> Result<u64, DataStoreError> {
        let find_options = FindOneOptions::builder().sort(doc! {"epoch": -1}).build();
        let validator = self
            .collection
            .find_one(doc! {}, find_options)
            .await?
            .expect("No entries found in validators table");
        Ok(validator.epoch)
    }

    pub async fn get_total_stake(&self, epoch: u64) -> Result<u64, DataStoreError> {
        let pipeline = vec![
            doc! {
                "$match": {
                    "epoch": epoch as u32,
                    "running_jito": true,
                }
            },
            doc! {
                "$group": {
                    "_id": bson::Bson::Null,
                    "total_stake_lamports": {
                        "$sum": "$active_stake"
                    }
                }
            },
        ];
        let mut cursor = self.collection.aggregate(pipeline, None).await?;
        if let Some(res) = cursor.try_next().await? {
            let doc: TotalStakeDbResult = bson::from_document(res)?;
            return Ok(doc.total_stake_lamports);
        }

        Err(DataStoreError::NoResultsFound)
    }

    pub async fn get_mev_commission_average_by_epoch(
        &self,
    ) -> Result<HashMap<u64, f64>, DataStoreError> {
        let pipeline = vec![
            doc! { "$match": { "running_jito": true } },
            doc! {
                "$group": {
                    "_id": {
                        "epoch": "$epoch",
                        "validator": "$vote_account"
                    },
                    "stakeWeightedMevComm": { "$sum": { "$multiply": [{ "$divide": ["$mev_commission_bps", 10000.0] }, "$active_stake"] } },
                    "totalStake": { "$sum": "$active_stake" }
                }
            },
            doc! {
                "$group": {
                    "_id": "$_id.epoch",
                    "totalStakeWeightedMevComm": { "$sum": "$stakeWeightedMevComm" },
                    "totalStake": { "$sum": "$totalStake" }
                }
            },
            doc! {
                "$project": {
                    "_id": 0,
                    "epoch": "$_id",
                    "weightedAvgMevComm": {
                        "$cond": {
                            "if": { "$eq": ["$totalStake", 0] },
                            "then": 0,
                            "else": { "$divide": ["$totalStakeWeightedMevComm", "$totalStake"] }
                        }
                    }
                }
            },
        ];
        let mut cursor = self.collection.aggregate(pipeline, None).await?;
        let mut results = HashMap::new();
        while let Ok(Some(result)) = cursor.try_next().await {
            let epoch = match result.get_i64("epoch") {
                Ok(epoch) => epoch as u64,
                Err(_) => continue,
            };
            let weighted_avg_mev_comm = match result.get_f64("weightedAvgMevComm") {
                Ok(weighted_avg_mev_comm) => weighted_avg_mev_comm,
                Err(_) => continue,
            };
            results.insert(epoch, weighted_avg_mev_comm);
        }
        if results.is_empty() {
            Err(DataStoreError::NoResultsFound)
        } else {
            Ok(results)
        }
    }

    pub async fn get_total_jito_stake_by_epoch(&self) -> Result<HashMap<u64, f64>, DataStoreError> {
        let pipeline = vec![
            doc! { "$match": { "running_jito": true } },
            doc! {
                "$group": {
                    "_id": {
                        "epoch": "$epoch",
                        "validator": "$vote_account"
                    },
                    "avgStake": { "$avg": "$stake_percent" },
                }
            },
            doc! {
                "$group": {
                    "_id": "$_id.epoch",
                    "stakeAmount": { "$sum": "$avgStake" }
                }
            },
        ];

        let mut cursor = self.collection.aggregate(pipeline, None).await?;
        let mut results = HashMap::new();
        while let Ok(Some(result)) = cursor.try_next().await {
            let epoch = result.get_i64("_id").unwrap_or_default() as u64;
            let stake_amount = result.get_f64("stakeAmount").unwrap_or_default();
            results.insert(epoch, stake_amount);
        }

        if results.is_empty() {
            Err(DataStoreError::NoResultsFound)
        } else {
            Ok(results)
        }
    }
}
