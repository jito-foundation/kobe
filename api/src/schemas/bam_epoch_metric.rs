use kobe_core::db_models::bam_epoch_metric::BamEpochMetric;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BamEpochMetricResponse {
    pub bam_epoch_metric: Option<BamEpochMetric>,
}
