use std::{collections::HashMap, str::FromStr};

use futures_util::TryStreamExt;
use mongodb::{
    bson,
    bson::doc,
    options::{FindOneOptions, FindOptions},
    Collection,
};
use serde::{Deserialize, Serialize};
use solana_pubkey::Pubkey;

use crate::{
    constants::{STAKER_REWARDS_COLLECTION_NAME, VALIDATOR_REWARDS_COLLECTION_NAME},
    db_models::error::DataStoreError,
    SortOrder,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ValidatorRewards {
    pub vote_account: String,
    pub mev_revenue: u64,
    pub priority_fee_revenue: Option<u64>,
    pub mev_commission: u16,
    pub priority_fee_commission: Option<u16>,
    pub num_stakers: u64,
    pub epoch: u64,
    pub claim_status_account: Option<String>,
}

#[derive(Clone)]
pub struct ValidatorRewardsStore {
    collection: Collection<ValidatorRewards>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MevRewardsDbResult {
    pub total_mev_revenue: u64,
}

impl ValidatorRewardsStore {
    pub const MAX_LIMIT: u32 = 10000;
    pub const COLLECTION: &'static str = VALIDATOR_REWARDS_COLLECTION_NAME;
    pub fn new(collection: Collection<ValidatorRewards>) -> Self {
        Self { collection }
    }

    pub async fn get_mev_rewards_sum(&self, epoch: u64) -> Result<u64, DataStoreError> {
        let pipeline = vec![
            doc! {
                "$match": {
                    "epoch": epoch as u32
                }
            },
            doc! {
                "$group": {
                    "_id": bson::Bson::Null,
                    "total_mev_revenue": {
                        "$sum": "$mev_revenue"
                    }
                }
            },
        ];

        let mut cursor = self.collection.aggregate(pipeline).await?;

        if let Some(res) = cursor.try_next().await? {
            let doc: MevRewardsDbResult = bson::deserialize_from_document(res)?;
            return Ok(doc.total_mev_revenue);
        }

        Err(DataStoreError::NoResultsFound)
    }

    // get mev rewards for all validators in a given epoch
    pub async fn get_mev_rewards_per_validator(
        &self,
        epoch: u64,
    ) -> Result<HashMap<String, u64>, DataStoreError> {
        let filter = doc! {
            "epoch": epoch as u32,
        };
        let find_options = FindOptions::builder().build();

        let mut cursor = self
            .collection
            .find(filter)
            .with_options(find_options)
            .await?;
        let mut results = HashMap::new();

        while let Some(res) = cursor.try_next().await? {
            results.insert(res.vote_account, res.mev_revenue);
        }

        Ok(results)
    }

    pub async fn get_validator_rewards(
        &self,
        vote_account: Option<&String>,
        epoch: Option<u64>,
        skip: Option<u64>,
        limit: Option<i64>,
        sort_order: Option<SortOrder>,
    ) -> Result<(Vec<ValidatorRewards>, u64), DataStoreError> {
        let mut filter = doc! {};

        if let Some(account) = vote_account {
            // validate it's a pubkey
            Pubkey::from_str(account.as_str())?;
            filter.insert("vote_account", account.to_string());
        }

        if let Some(e) = epoch {
            filter.insert("epoch", e as u32);
        }

        let total_count = self.collection.count_documents(filter.clone()).await?;

        let sort_direction = match sort_order {
            Some(SortOrder::Asc) => 1,
            _ => -1, // Default to descending
        };

        let find_options = FindOptions::builder()
            .sort(doc! {
                "epoch": sort_direction,
                "mev_revenue": sort_direction
            })
            .skip(skip)
            .limit(limit)
            .build();

        let mut cursor = self
            .collection
            .find(filter)
            .with_options(find_options)
            .await?;
        let mut results = Vec::new();
        while let Some(res) = cursor.try_next().await? {
            results.push(res);
        }
        Ok((results, total_count))
    }

    pub async fn get_highest_epoch(&self) -> Result<u64, DataStoreError> {
        let find_options = FindOneOptions::builder().sort(doc! {"epoch": -1}).build();
        let validator_rewards = self
            .collection
            .find_one(doc! {})
            .with_options(find_options)
            .await?
            .expect("No entries found in validator rewards table");
        Ok(validator_rewards.epoch)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct StakerRewards {
    pub claimant: String,
    pub stake_authority: String,
    pub withdraw_authority: String,
    pub validator_vote_account: String,
    pub epoch: u64,
    pub amount: u64,
    pub priority_fee_amount: Option<u64>,
    pub claim_status_account: Option<String>,
    pub priority_fee_claim_status_account: Option<String>,
}

#[derive(Clone)]
pub struct StakerRewardsStore {
    collection: Collection<StakerRewards>,
}

impl StakerRewardsStore {
    pub const MAX_LIMIT: u32 = 10000;
    pub const COLLECTION: &'static str = STAKER_REWARDS_COLLECTION_NAME;
    pub fn new(collection: Collection<StakerRewards>) -> Self {
        Self { collection }
    }

    pub async fn get_staker_rewards(
        &self,
        staker_authority: Option<&str>,
        validator_vote_account: Option<&str>,
        epoch: Option<u64>,
        skip: Option<u64>,
        limit: Option<i64>,
        sort_order: Option<SortOrder>,
    ) -> Result<(Vec<StakerRewards>, u64), DataStoreError> {
        let mut filter = doc! {};

        if let Some(authority) = staker_authority {
            // Validate it's a pubkey
            Pubkey::from_str(authority)?;
            filter.insert("stake_authority", authority.to_string());
        }

        if let Some(vote_account) = validator_vote_account {
            // Validate it's a pubkey
            Pubkey::from_str(vote_account)?;
            filter.insert("validator_vote_account", vote_account.to_string());
        }

        if let Some(e) = epoch {
            filter.insert("epoch", e as u32);
        }

        let num_filters = {
            let mut count = 0;
            if staker_authority.is_some() {
                count += 1;
            }
            if validator_vote_account.is_some() {
                count += 1;
            }
            if epoch.is_some() {
                count += 1;
            }
            count
        };

        let total_count = if num_filters > 2 {
            self.collection.count_documents(filter.clone()).await?
        } else {
            0
        };

        let sort_direction = match sort_order {
            Some(SortOrder::Asc) => 1,
            Some(SortOrder::Desc) => -1,
            None => -1, // Default to descending if not specified
        };

        let find_options_builder = FindOptions::builder()
            .sort(doc! {"epoch": sort_direction, "amount": sort_direction})
            .skip(skip)
            .limit(limit);

        let find_options = find_options_builder.build();

        let mut cursor = self
            .collection
            .find(filter)
            .with_options(find_options)
            .await?;
        let mut results = Vec::new();
        while let Some(res) = cursor.try_next().await? {
            results.push(res);
        }
        Ok((results, total_count))
    }
}
