use serde::{Deserialize, Serialize};

/// Validator Information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorsResponse {
    /// Validator node pubkey
    pub validator_pubkey: String,

    /// BAM node connection
    pub bam_node_connection: String,

    /// BAM stake
    pub stake: f64,

    /// Stake percentage
    pub stake_percentage: f64,
}
