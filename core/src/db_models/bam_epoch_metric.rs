use mongodb::{bson::DateTime, Collection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BamEpochMetric {
    /// Epoch number
    epoch: u64,

    /// BAM total network stake weight
    bam_total_network_stake_weight: f64,

    /// Available BAM delegation stake
    available_bam_delegation_stake: u64,

    /// Eligible BAM validator count
    eligible_bam_validator_count: u64,

    /// Timestamp
    timestamp: Option<DateTime>,
}

impl BamEpochMetric {
    pub fn new(
        epoch: u64,
        bam_total_network_stake_weight: f64,
        available_bam_delegation_stake: u64,
        eligible_bam_validator_count: u64,
    ) -> Self {
        Self {
            epoch,
            bam_total_network_stake_weight,
            available_bam_delegation_stake,
            eligible_bam_validator_count,
            timestamp: Some(DateTime::now()),
        }
    }
}

#[derive(Clone)]
pub struct BamEpochMetricStore {
    /// Collection of BamEpochMetrics
    collection: Collection<BamEpochMetric>,
}

impl BamEpochMetricStore {
    pub const COLLECTION: &'static str = "bam_epoch_metrics";

    /// Initialize a [`BamEpochMetricStore`]
    pub fn new(collection: Collection<BamEpochMetric>) -> Self {
        Self { collection }
    }

    /// Insert a [`BamEpochMetric`] record
    pub async fn insert(
        &self,
        bam_epoch_metric: BamEpochMetric,
    ) -> Result<(), mongodb::error::Error> {
        self.collection.insert_one(bam_epoch_metric, None).await?;
        Ok(())
    }
}
