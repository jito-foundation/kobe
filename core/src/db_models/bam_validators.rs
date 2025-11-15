//! DB model for a BAM validator.

use std::{str::FromStr, time::Instant};

use chrono::{serde::ts_seconds, DateTime, Utc};
use futures::TryStreamExt;
use mongodb::{bson::doc, Collection};
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

    /// Find [`BamValidator`] records
    pub async fn find(&self, epoch: u64) -> Result<Vec<BamValidator>, DataStoreError> {
        let filter = doc! {"epoch": epoch as u32};

        let cursor = self.collection.find(filter, None).await?;
        let validators: Vec<BamValidator> = cursor.try_collect().await?;

        Ok(validators)
    }
}
