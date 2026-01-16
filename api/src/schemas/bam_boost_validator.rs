use kobe_core::db_models::bam_boost_validators::BamBoostValidator;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct BamBoostValidatorsRequest {
    pub epoch: u64,
}

impl std::fmt::Display for BamBoostValidatorsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.epoch)
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct BamBoostValidatorsResponse {
    pub bam_boost_validators: Vec<BamBoostValidator>,
}
