use mongodb::{
    bson::{self, doc},
    options::FindOneOptions,
    Collection,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamEpochMetric {
    /// Epoch number
    pub epoch: u64,

    /// BAM total stake weight
    pub bam_total_stake_weight: u64,

    /// Available BAM delegation stake
    pub available_bam_delegation_stake: u64,

    /// Eligible BAM validator count
    pub eligible_bam_validator_count: u64,
}

impl BamEpochMetric {
    pub fn new(
        epoch: u64,
        bam_total_stake_weight: u64,
        available_bam_delegation_stake: u64,
        eligible_bam_validator_count: u64,
    ) -> Self {
        Self {
            epoch,
            bam_total_stake_weight,
            available_bam_delegation_stake,
            eligible_bam_validator_count,
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

    /// Upsert a [`BamEpochMetric`] record
    pub async fn upsert(
        &self,
        bam_epoch_metric: BamEpochMetric,
    ) -> Result<(), mongodb::error::Error> {
        let update = doc! { "$set": bson::to_document(&bam_epoch_metric)? };
        let filter = doc! { "epoch": bam_epoch_metric.epoch as u32 };
        let options = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();
        self.collection.update_one(filter, update, options).await?;
        Ok(())
    }

    /// Find a [`BamEpochMetric`] record by epoch
    pub async fn find_by_epoch(
        &self,
        epoch: Option<u64>,
    ) -> Result<Option<BamEpochMetric>, mongodb::error::Error> {
        match epoch {
            Some(epoch) => {
                self.collection
                    .find_one(doc! {"epoch": epoch as u32}, None)
                    .await
            }
            None => {
                self.collection
                    .find_one(
                        doc! {},
                        FindOneOptions::builder().sort(doc! {"epoch": -1}).build(),
                    )
                    .await
            }
        }
    }
}
