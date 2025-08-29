use std::collections::HashMap;
use std::str::FromStr;

use kobe_core::{
    constants::{STAKER_REWARDS_COLLECTION_NAME, VALIDATOR_REWARDS_COLLECTION_NAME},
    db_models::mev_rewards::{StakerRewards, ValidatorRewards},
};
use log::debug;
use mongodb::Database;
use solana_sdk::{clock::Epoch, pubkey::Pubkey};

use crate::{
    db::write_to_db,
    result::{AppError, Result},
    tip_distributor_sdk::{GeneratedMerkleTreeCollection, StakeMetaCollection},
};

pub async fn parse_merkle_tree(
    db: &Database,
    target_epoch: Epoch,
    merkle_tree_collection: &GeneratedMerkleTreeCollection,
    stake_meta_collection: &StakeMetaCollection,
    tip_distribution_program_id: &str,
    priority_fee_distribution_program_id: &str,
) -> Result<()> {
    if stake_meta_collection.epoch != target_epoch {
        panic!("Problem with the stake meta upload. Epochs do not match")
    }
    if merkle_tree_collection.epoch != target_epoch {
        panic!("Problem with the merkle tree upload. Epochs do not match")
    }

    // Use stake meta to create inverse validator mapping
    #[derive(Debug, Clone)]
    struct ValidatorMeta {
        vote_account: Pubkey,
        mev_commission: u16,
        priority_fee_commission: Option<u16>,
        epoch: Epoch,
    }

    #[derive(Debug, Clone)]
    struct ValidatorRewardsBuilder {
        vote_account: Option<Pubkey>,
        mev_commission: Option<u16>,
        priority_fee_commission: Option<u16>,
        epoch: Option<Epoch>,
        mev_revenue: Option<u64>,
        priority_fee_revenue: Option<u64>,
        num_stakers: Option<u64>,
        claim_status_account: Option<String>,
    }

    impl ValidatorRewardsBuilder {
        fn default() -> Self {
            Self {
                vote_account: None,
                mev_commission: None,
                priority_fee_commission: None,
                epoch: None,
                mev_revenue: None,
                priority_fee_revenue: None,
                num_stakers: None,
                claim_status_account: None,
            }
        }

        fn build(&self) -> ValidatorRewards {
            ValidatorRewards {
                vote_account: self.vote_account.unwrap().to_string(),
                mev_commission: self.mev_commission.unwrap(),
                priority_fee_commission: self.priority_fee_commission,
                epoch: self.epoch.unwrap(),
                mev_revenue: self.mev_revenue.unwrap_or(0),
                priority_fee_revenue: self.priority_fee_revenue,
                num_stakers: self.num_stakers.unwrap_or(0),
                claim_status_account: self.claim_status_account.clone(),
            }
        }
    }

    #[derive(Debug, Clone)]
    struct StakerRewardsBuilder {
        validator_meta: Option<ValidatorMeta>,
        epoch: Option<Epoch>,
        claimant: Option<Pubkey>,
        stake_authority: Option<Pubkey>,
        withdraw_authority: Option<Pubkey>,
        validator_vote_account: Option<Pubkey>,
        tip_amount: Option<u64>,
        tip_claim_status_account: Option<String>,
        priority_fee_amount: Option<u64>,
        priority_fee_claim_status_account: Option<String>,
    }

    impl StakerRewardsBuilder {
        fn default() -> Self {
            Self {
                validator_meta: None,
                epoch: None,
                claimant: None,
                stake_authority: None,
                withdraw_authority: None,
                validator_vote_account: None,
                tip_amount: None,
                tip_claim_status_account: None,
                priority_fee_amount: None,
                priority_fee_claim_status_account: None,
            }
        }

        fn build(&self) -> StakerRewards {
            StakerRewards {
                claimant: self.claimant.unwrap().to_string(),
                stake_authority: self.stake_authority.unwrap().to_string(),
                withdraw_authority: self.withdraw_authority.unwrap().to_string(),
                validator_vote_account: self.validator_vote_account.unwrap().to_string(),
                epoch: self.epoch.unwrap(),
                amount: self.tip_amount.unwrap_or(0),
                priority_fee_amount: self.priority_fee_amount,
                claim_status_account: self.tip_claim_status_account.clone(),
                priority_fee_claim_status_account: self.priority_fee_claim_status_account.clone(),
            }
        }
    }

    let tda_to_validator: HashMap<Pubkey, ValidatorMeta> =
        HashMap::from_iter(stake_meta_collection.stake_metas.iter().filter_map(|meta| {
            meta.maybe_tip_distribution_meta
                .as_ref()
                .map(|tip_distribution_meta| {
                    (
                        tip_distribution_meta.tip_distribution_pubkey,
                        ValidatorMeta {
                            vote_account: meta.validator_vote_account,
                            mev_commission: tip_distribution_meta.validator_fee_bps,
                            priority_fee_commission: meta
                                .maybe_priority_fee_distribution_meta
                                .as_ref()
                                .map(|priority_fee_meta| priority_fee_meta.validator_fee_bps),
                            epoch: target_epoch,
                        },
                    )
                })
        }));

    // Parse files
    let mut validators_to_write = vec![];
    let mut stakers_to_write = vec![];
    let validator_collection = db.collection::<ValidatorRewards>(VALIDATOR_REWARDS_COLLECTION_NAME);
    let staker_collection = db.collection::<StakerRewards>(STAKER_REWARDS_COLLECTION_NAME);

    let mut validator_rewards: HashMap<Pubkey, ValidatorRewardsBuilder> = HashMap::new();
    let mut staker_rewards: HashMap<Pubkey, StakerRewardsBuilder> = HashMap::new();

    let priority_fee_distribution_pubkey =
        Pubkey::from_str(priority_fee_distribution_program_id).unwrap();
    let tip_distribution_pubkey = Pubkey::from_str(tip_distribution_program_id).unwrap();

    for tree in merkle_tree_collection.generated_merkle_trees.iter() {
        let maybe_validator = tda_to_validator.get(&tree.distribution_account);
        if let Some(validator) = maybe_validator {
            if validator.epoch != target_epoch {
                return Err(AppError::MalformedMerkleTreeError);
            }
            let mut validator_reward =
                if let Some(rewards) = validator_rewards.get(&validator.vote_account) {
                    rewards.to_owned()
                } else {
                    ValidatorRewardsBuilder::default()
                };
            tree.tree_nodes.iter().for_each(|node| {
                let mut claimant_reward = if let Some(rewards) = staker_rewards.get(&node.claimant)
                {
                    rewards.to_owned()
                } else {
                    StakerRewardsBuilder::default()
                };
                validator_reward.num_stakers = Some(tree.tree_nodes.len() as u64);
                if tree.distribution_program
                    == Pubkey::from_str(tip_distribution_program_id).unwrap()
                {
                    if node.staker_pubkey == Pubkey::default() {
                        validator_reward.claim_status_account =
                            Some(node.claim_status_pubkey.to_string());
                    }
                    claimant_reward.tip_amount = Some(node.amount);
                    claimant_reward.tip_claim_status_account =
                        Some(node.claim_status_pubkey.to_string());
                } else if tree.distribution_program
                    == Pubkey::from_str(priority_fee_distribution_program_id).unwrap()
                {
                    claimant_reward.priority_fee_amount = Some(node.amount);
                    claimant_reward.priority_fee_claim_status_account =
                        Some(node.claim_status_pubkey.to_string());
                } else {
                    panic!("Unknown distribution program");
                }
                claimant_reward.validator_meta = Some(validator.clone());
                claimant_reward.epoch = Some(target_epoch);
                claimant_reward.claimant = Some(node.claimant);
                claimant_reward.stake_authority = Some(node.staker_pubkey);
                claimant_reward.withdraw_authority = Some(node.withdrawer_pubkey);
                claimant_reward.validator_vote_account = Some(validator.vote_account);
                staker_rewards.insert(node.claimant, claimant_reward);
            });

            if tree.distribution_program == tip_distribution_pubkey {
                validator_reward.mev_revenue = Some(tree.max_total_claim);
            } else if tree.distribution_program == priority_fee_distribution_pubkey {
                validator_reward.priority_fee_revenue = Some(tree.max_total_claim);
            } else {
                panic!("Unknown distribution program");
            }
            validator_reward.priority_fee_commission = validator.priority_fee_commission;
            validator_reward.mev_commission = Some(validator.mev_commission);
            validator_reward.vote_account = Some(validator.vote_account);
            validator_reward.epoch = Some(target_epoch);
            validator_rewards.insert(validator.vote_account, validator_reward);
        } else {
            debug!(
                "Did not find validator in stake meta for tip distribution acc {}",
                tree.distribution_account
            );
        }
    }

    for (_, v) in validator_rewards.into_iter() {
        validators_to_write.push(v.build());
    }

    for (_, v) in staker_rewards.into_iter() {
        stakers_to_write.push(v.build());
    }

    write_to_db(&staker_collection, &stakers_to_write).await?;
    write_to_db(&validator_collection, &validators_to_write).await?;
    Ok(())
}
