//! DB model for a BAM epoch metrics.

use chrono::{serde::ts_seconds, DateTime, Utc};
use mongodb::{
    bson::{self, doc},
    Collection,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamEpochMetrics {
    /// Allocation tier based on JIP-28 in BPS
    allocation_bps: u64,

    /// Total JitoSOL stake available for BAM delegation in lamports.
    /// This is calculated as: (current_tier_allocation_bps / 10000) * total_jitosol_stake
    /// Represents the portion of JitoSOL that will be delegated to validators based on
    /// their percentage of active stake currently running BAM.
    available_bam_delegation_stake: u64,

    /// Total stake amount of BAM-running validators in lamports
    bam_stake: u64,

    /// Eligible BAM validator count
    eligible_bam_validator_count: u64,

    /// Epoch number
    epoch: u64,

    /// Total JitoSOL TVL in lamports
    jitosol_stake: u64,

    /// Timestamp
    #[serde(with = "ts_seconds")]
    timestamp: DateTime<Utc>,

    /// Total stake amount of all validators in lamports
    total_stake: u64,
}

impl BamEpochMetrics {
    pub fn new(
        epoch: u64,
        bam_stake: u64,
        total_stake: u64,
        jitosol_stake: u64,
        eligible_bam_validator_count: u64,
    ) -> Self {
        let timestamp = Utc::now();

        Self {
            allocation_bps: 0,
            available_bam_delegation_stake: 0,
            bam_stake,
            eligible_bam_validator_count,
            epoch,
            jitosol_stake,
            timestamp,
            total_stake,
        }
    }

    /// Set allocation percentage in BPS
    pub fn set_allocation_bps(&mut self, allocation_bps: u64) {
        self.allocation_bps = allocation_bps;
    }

    /// Get available bam delegation stake in lamports
    pub fn get_available_bam_delegation_stake(&self) -> u64 {
        self.available_bam_delegation_stake
    }

    /// Set available bam delegation stake in lamports
    pub fn set_available_bam_delegation_stake(&mut self, available_bam_delegation_stake: u64) {
        self.available_bam_delegation_stake = available_bam_delegation_stake;
    }

    /// Get bam stake amount in lamports
    pub fn get_bam_stake(&self) -> u64 {
        self.bam_stake
    }

    /// Get epoch number
    pub fn get_epoch(&self) -> u64 {
        self.epoch
    }

    /// Get JitoSOL stake
    pub fn get_jitosol_stake(&self) -> u64 {
        self.jitosol_stake
    }

    /// Set JitoSOL stake
    pub fn set_jitosol_stake(&mut self, jitosol_stake: u64) {
        self.jitosol_stake = jitosol_stake;
    }

    /// Get total stake amount in lamports
    pub fn get_total_stake(&self) -> u64 {
        self.total_stake
    }
}

#[derive(Clone)]
pub struct BamEpochMetricsStore {
    /// Collection of BamEpochMetrics
    collection: Collection<BamEpochMetrics>,
}

impl BamEpochMetricsStore {
    pub const COLLECTION: &'static str = "bam_epoch_metrics";

    /// Initialize a [`BamEpochMetricStore`]
    pub fn new(collection: Collection<BamEpochMetrics>) -> Self {
        Self { collection }
    }

    /// Insert a [`BamEpochMetrics`] record
    pub async fn insert(
        &self,
        bam_epoch_metrics: BamEpochMetrics,
    ) -> Result<(), mongodb::error::Error> {
        self.collection.insert_one(bam_epoch_metrics, None).await?;
        Ok(())
    }

    /// Upsert a [`BamEpochMetrics`] record
    pub async fn upsert(
        &self,
        bam_epoch_metrics: BamEpochMetrics,
    ) -> Result<(), mongodb::error::Error> {
        let update = doc! { "$set": bson::to_document(&bam_epoch_metrics)? };
        let filter = doc! { "epoch": bam_epoch_metrics.epoch as u32 };
        let options = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();
        self.collection.update_one(filter, update, options).await?;
        Ok(())
    }

    /// Find a [`BamEpochMetrics`] record by epoch
    pub async fn find_by_epoch(
        &self,
        epoch: u64,
    ) -> Result<Option<BamEpochMetrics>, mongodb::error::Error> {
        self.collection
            .find_one(doc! {"epoch": epoch as u32}, None)
            .await
    }
}
