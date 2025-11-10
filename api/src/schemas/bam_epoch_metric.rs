use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BamEpochMetricResponse {
    /// Epoch number
    epoch: u64,

    /// BAM total network stake weight
    bam_total_network_stake_weight: u64,

    /// Available BAM delegation stake
    available_bam_delegation_stake: u64,

    /// Eligible BAM validator count
    eligible_bam_validator_count: u64,

    /// Timestamp
    timestamp: Option<chrono::DateTime<chrono::Utc>>,
}
