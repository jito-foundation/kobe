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

#[derive(Serialize, Deserialize, Clone)]
pub struct BamValidatorRequest {
    /// Epoch number
    pub epoch: u64,

    /// Vote account
    pub vote_account: String,
}

impl std::fmt::Display for BamValidatorRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.epoch)
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct BamValidatorResponse {
    pub bam_validator: Option<BamValidator>,
}
