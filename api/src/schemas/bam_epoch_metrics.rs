use kobe_core::db_models::bam_epoch_metrics::BamEpochMetrics;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct BamEpochMetricsRequest {
    pub epoch: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BamEpochMetricsResponse {
    pub bam_epoch_metrics: Option<BamEpochMetrics>,
}
