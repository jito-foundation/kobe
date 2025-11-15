use kobe_core::db_models::bam_validators::BamValidator;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct BamValidatorsRequest {
    pub epoch: u64,
}

impl std::fmt::Display for BamValidatorsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.epoch)
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct BamValidatorsResponse {
    pub bam_validators: Vec<BamValidator>,
}
