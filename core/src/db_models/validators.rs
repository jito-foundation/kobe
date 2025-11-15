//! DB model for a validator.
//!
//! The initial implementation uses multiple fields sourced from
//! Validators.app, however we will iteratively replace components with our own
//! On-chain derived metrics.

use std::{collections::HashMap, str::FromStr};

use chrono::{serde::ts_seconds, DateTime, Utc};
use futures::TryStreamExt;
use mongodb::{bson, bson::doc, options::FindOneOptions, Collection};
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;

use crate::{
    constants::VALIDATOR_COLLECTION_NAME, db_models::error::DataStoreError, fetcher::ChainData,
    validators_app::ValidatorsAppResponseEntry,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Validator {
    pub active_stake: Option<u64>,
    pub commission: Option<u8>,
    pub consensus_mods_score: Option<i8>,
    pub data_center_concentration_score: Option<i64>,
    pub delinquent: Option<bool>,
    pub epoch: u64,
    pub epoch_credits: Option<u64>,
    pub identity_account: Option<String>,

    pub mev_commission_bps: Option<u16>,
    pub mev_revenue_lamports: u64,
    pub priority_fee_commission_bps: Option<u16>,
    pub priority_fee_revenue_lamports: Option<u64>,
    pub name: Option<String>,
    pub published_information_score: Option<i64>,
    pub root_distance_score: Option<i64>,
    /// Whether or not running Jito client
    pub running_jito: bool,

    /// Whether or not running BAM client
    pub running_bam: Option<bool>,
    pub software_version: Option<String>,
    pub software_version_score: Option<i64>,
    pub skipped_slot_percent: Option<String>,
    pub skipped_slot_score: Option<i64>,
    pub skipped_slots: Option<u64>,
    pub stake_concentration_score: Option<i64>,
    pub stake_percent: f64,
    // Target pool is the pool this cranker is managing
    pub target_pool_active_lamports: u64,
    pub target_pool_transient_lamports: u64,
    pub target_pool_staked: bool,
    #[serde(with = "ts_seconds")]
    pub timestamp: DateTime<Utc>,
    pub vote_account: String,
    pub vote_credit_proportion: f64,
    pub www_url: Option<String>,
    pub inflation_rewards_lamports: u64,
}

impl Validator {
    pub fn new(
        validators_app_entry: &ValidatorsAppResponseEntry,
        on_chain_data: &ChainData,
    ) -> Self {
        let (active_stake, transient_stake) = if let Some(stake_info) = on_chain_data.stake_info {
            (
                stake_info.active_stake_lamports.into(),
                stake_info.transient_stake_lamports.into(),
            )
        } else {
            (0, 0)
        };
        let timestamp = Utc::now();

        Self {
            active_stake: validators_app_entry.active_stake,
            commission: validators_app_entry.commission,
            consensus_mods_score: validators_app_entry.consensus_mods_score,
            data_center_concentration_score: validators_app_entry.data_center_concentration_score,
            delinquent: validators_app_entry.delinquent,
            epoch: validators_app_entry.epoch.unwrap_or_default(),
            epoch_credits: validators_app_entry.epoch_credits,
            identity_account: validators_app_entry.account.clone(),
            mev_commission_bps: on_chain_data.mev_commission_bps,
            mev_revenue_lamports: on_chain_data.mev_revenue_lamports,
            priority_fee_commission_bps: Some(on_chain_data.priority_fee_commission_bps),
            priority_fee_revenue_lamports: Some(on_chain_data.priority_fee_revenue_lamports),
            name: validators_app_entry.name.clone(),
            published_information_score: validators_app_entry.published_information_score,
            root_distance_score: validators_app_entry.root_distance_score,
            running_jito: on_chain_data.running_jito,
            running_bam: Some(on_chain_data.running_bam),
            software_version: validators_app_entry.software_version.clone(),
            software_version_score: validators_app_entry.software_version_score,
            skipped_slot_percent: validators_app_entry.skipped_slot_percent.clone(),
            skipped_slot_score: validators_app_entry.skipped_slot_score,
            skipped_slots: validators_app_entry.skipped_slots,
            stake_concentration_score: validators_app_entry.stake_concentration_score,
            stake_percent: validators_app_entry.active_stake.unwrap_or_default() as f64
                / on_chain_data.total_staked_lamports as f64,
            target_pool_active_lamports: active_stake,
            target_pool_transient_lamports: transient_stake,
            target_pool_staked: on_chain_data.stake_info.is_some(),
            timestamp,
            vote_account: validators_app_entry.vote_account.to_string(),
            vote_credit_proportion: on_chain_data.vote_credit_proportion,
            www_url: validators_app_entry.www_url.clone(),
            inflation_rewards_lamports: on_chain_data.inflation_rewards_lamports,
        }
    }

    pub fn vote_account(&self) -> Pubkey {
        Pubkey::from_str(self.vote_account.as_str()).unwrap()
    }

    /// Get the total lamports delegated to this validator (active and transient)
    // Copied from spl_stake_pool::state::ValidatorStakeInfo
    pub fn stake_lamports(&self) -> u64 {
        self.target_pool_active_lamports
            .checked_add(self.target_pool_transient_lamports)
            .unwrap()
    }
}

#[derive(Clone)]
pub struct ValidatorStore {
    collection: Collection<Validator>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TotalStakeDbResult {
    pub total_stake_lamports: u64,
}

impl ValidatorStore {
    pub const COLLECTION: &'static str = VALIDATOR_COLLECTION_NAME;

    pub fn new(collection: Collection<Validator>) -> Self {
        Self { collection }
    }

    pub async fn find(
        &self,
        epoch: u64,
        only_jito: bool,
    ) -> Result<Vec<Validator>, DataStoreError> {
        let filter = if only_jito {
            doc! {
                "epoch": {"$in" :vec![epoch as u32, (epoch-1) as u32]},
                "running_jito": true
            }
        } else {
            doc! {
                "epoch": {"$in" :vec![epoch as u32, (epoch-1) as u32]},
            }
        };
        // Jito filter fetch requires extra epoch lookback due to delay in publishing tip distr acc
        let cursor = self.collection.find(filter, None).await?;
        let validators: Vec<Validator> = cursor.try_collect().await?;
        let mut validators_map: HashMap<String, Validator> = HashMap::new();

        // Loop through all validators and return the most recent entry in which running_jito is true
        // Or add all if we don't need to filter
        for v in validators.into_iter() {
            if let Some(entry) = validators_map.get(&v.vote_account) {
                if entry.timestamp < v.timestamp {
                    validators_map.insert(v.vote_account.clone(), v);
                }
            } else {
                validators_map.insert(v.vote_account.clone(), v);
            }
        }

        Ok(validators_map.into_values().collect::<Vec<Validator>>())
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
