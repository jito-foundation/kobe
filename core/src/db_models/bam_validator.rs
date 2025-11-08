//! DB model for a BAM Validator.

use mongodb::{
    bson::{self, doc},
    Collection,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BamValidator {
    /// Epoch number
    epoch: u64,

    /// Vote account
    vote_account: String,

    /// Available delegation stake
    available_delegation_stake: f64,

    /// Is eligible validator
    is_eligible: bool,

    /// Is directed stake target
    is_directed_stake_target: bool,
}

impl BamValidator {
    pub fn new(
        epoch: u64,
        vote_account: String,
        available_delegation_stake: f64,
        is_eligible: bool,
        is_directed_stake_target: bool,
    ) -> Self {
        Self {
            epoch,
            vote_account,
            available_delegation_stake,
            is_eligible,
            is_directed_stake_target,
        }
    }
}

#[derive(Clone)]
pub struct BamValidatorStore {
    collection: Collection<BamValidator>,
}

impl BamValidatorStore {
    pub const COLLECTION: &'static str = "bam_validators";

    /// Initialize a [`BamValidatorStore`]
    pub fn new(collection: Collection<BamValidator>) -> Self {
        Self { collection }
    }

    /// Insert a [`BamValidator`] record
    pub async fn insert(&self, validator: BamValidator) -> Result<(), mongodb::error::Error> {
        self.collection.insert_one(validator, None).await?;
        Ok(())
    }

    /// Upsert a [`BamValidator`] record
    pub async fn upsert(&self, validator: BamValidator) -> Result<(), mongodb::error::Error> {
        let update = doc! { "$set": bson::to_document(&validator)? };
        let filter = doc! { "vote_account": &validator.vote_account  };
        let options = mongodb::options::UpdateOptions::builder()
            .upsert(true)
            .build();
        self.collection.update_one(filter, update, options).await?;
        Ok(())
    }

    /// Upsert multiple [`BamValidator`] records
    pub async fn bulk_upsert(
        &self,
        validators: Vec<BamValidator>,
    ) -> Result<(), mongodb::error::Error> {
        for validator in validators {
            self.upsert(validator).await?;
        }
        Ok(())
    }
}
