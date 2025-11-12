use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Daily MEV tips data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyMevRewards {
    /// Date of the tips
    pub day: DateTime<Utc>,

    /// Number of MEV tips
    pub count_mev_tips: u64,

    /// Jito tips amount (SOL)
    pub jito_tips: f64,

    /// Number of unique tippers
    pub tippers: u64,

    /// Validator tips amount (SOL)
    pub validator_tips: f64,
}
