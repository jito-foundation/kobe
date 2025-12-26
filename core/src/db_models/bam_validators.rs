//! DB model for a BAM validator.

use std::{str::FromStr, time::Instant};

use chrono::{serde::ts_seconds, DateTime, Utc};
use futures::TryStreamExt;
use mongodb::{bson::doc, options::ReplaceOptions, Collection};
use serde::{Deserialize, Serialize};
use solana_pubkey::{ParsePubkeyError, Pubkey};

use crate::db_models::error::DataStoreError;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct BamValidator {
    /// Active stake in lamports
    active_stake: u64,

    /// Epoch number
    epoch: u64,

    /// Identity account public key
    identity_account: String,

    /// Is eligible validator
    is_eligible: bool,

    /// The reason of ineligibility
    ineligibility_reason: Option<String>,

    /// BAM delegation scoring
    ///
    /// Validators with a score of 0 will receive a target delegation of 0 lamports when next updating the directed stake meta
    score: Option<u8>,

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
            is_eligible: false,
            ineligibility_reason: None,
            score: None,
            timestamp,
            vote_account: String::new(),
        }
    }
}

impl BamValidator {
    /// Initialize a [`BamValidator`]
    pub fn new(
        active_stake: u64,
        epoch: u64,
        identity_account: &str,
        is_eligible: bool,
        vote_account: &str,
    ) -> Self {
        let timestamp = Utc::now();

        Self {
            active_stake,
            epoch,
            identity_account: identity_account.to_string(),
            is_eligible,
            ineligibility_reason: None,
            score: Some(0),
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

    /// Eligible validator
    pub fn is_eligible(&self) -> bool {
        self.is_eligible
    }

    /// Set is eligible validator
    pub fn set_is_eligible(&mut self, is_eligible: bool) {
        self.is_eligible = is_eligible;
    }

    /// Get ineligiblity reason
    pub fn set_ineligibility_reason(&mut self, ineligibility_reason: Option<String>) {
        self.ineligibility_reason = ineligibility_reason;
    }

    /// Get identity account pubkey (node public key)
    pub fn get_identity_account(&self) -> Result<Pubkey, ParsePubkeyError> {
        Pubkey::from_str(&self.identity_account)
    }

    /// Set identity account public key
    pub fn set_identity_account(&mut self, identity_account: String) {
        self.identity_account = identity_account;
    }

    /// Set BAM delegation scoring
    pub fn set_score(&mut self, score: u8) {
        self.score = Some(score);
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
    pub const COLLECTION: &'static str = "bam_validators";

    /// Initialize a [`BamValidatorStore`]
    pub fn new(collection: Collection<BamValidator>) -> Self {
        Self { collection }
    }

    /// Insert many [`BamValidator`]
    pub async fn insert_many(&self, items: &[BamValidator]) -> Result<(), DataStoreError> {
        if items.is_empty() {
            log::info!("No items to insert");
            return Ok(());
        }

        let start = Instant::now();
        self.collection.insert_many(items, None).await?;

        log::info!(
            "done writing {:#?} items to db, took {}ms",
            items.len(),
            start.elapsed().as_millis()
        );

        Ok(())
    }

    /// Upsert a [`BamEpochMetrics`] record
    pub async fn upsert(
        &self,
        items: &[BamValidator],
        epoch: u64,
    ) -> Result<(), mongodb::error::Error> {
        let batch_size = 100;

        let mut replace_options = ReplaceOptions::default();
        replace_options.upsert = Some(true);

        for chunk in items.chunks(batch_size) {
            for item in chunk {
                self.collection
                    .replace_one(
                        doc! {
                            "epoch": epoch as u32,
                            "vote_account": &item.vote_account
                        },
                        item,
                        replace_options.clone(),
                    )
                    .await?;
            }

            // Small delay between batches to avoid overwhelming the server
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

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
