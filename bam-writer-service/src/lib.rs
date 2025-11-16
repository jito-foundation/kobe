use std::{collections::HashMap, str::FromStr, sync::Arc};

use bam_api_client::client::BamApiClient;
use kobe_core::db_models::{
    bam_epoch_metric::{BamEpochMetric, BamEpochMetricStore},
    bam_validators::{BamValidator, BamValidatorStore},
};
use mongodb::Collection;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_pubkey::Pubkey;
use stakenet_sdk::utils::accounts::{get_all_validator_history_accounts, get_stake_pool_account};

use crate::{
    bam_delegation_criteria::BamDelegationCriteria,
    bam_validator_eligibility::BamValidatorEligibility,
};

mod bam_delegation_criteria;
mod bam_validator_eligibility;

pub struct BamWriterService {
    /// Stake pool address
    stake_pool: Pubkey,

    /// RPC Client
    rpc_client: Arc<RpcClient>,

    /// BAM API client
    bam_api_client: BamApiClient,

    /// Bam validators store
    bam_validators_store: BamValidatorStore,

    /// Bam epoch metric store
    bam_epoch_metric_store: BamEpochMetricStore,

    /// BAM Delegation Criteria
    bam_delegation_criteria: BamDelegationCriteria,
}

impl BamWriterService {
    /// Initialize [`BamWriterService`]
    pub async fn new(
        mongo_connection_uri: &str,
        mongo_db_name: &str,
        stake_pool: Pubkey,
        rpc_client: Arc<RpcClient>,
        bam_api_base_url: &str,
    ) -> anyhow::Result<Self> {
        // Connect to MongoDB
        let client = mongodb::Client::with_uri_str(mongo_connection_uri).await?;
        let db = client.database(mongo_db_name);

        let bam_validators_collection: Collection<BamValidator> =
            db.collection(BamValidatorStore::COLLECTION);
        let bam_validators_store = BamValidatorStore::new(bam_validators_collection);

        let bam_epoch_metric_collection: Collection<BamEpochMetric> =
            db.collection(BamEpochMetricStore::COLLECTION);
        let bam_epoch_metric_store = BamEpochMetricStore::new(bam_epoch_metric_collection);

        let bam_api_config = bam_api_client::config::Config::custom(bam_api_base_url);
        let bam_api_client = BamApiClient::new(bam_api_config);

        let bam_delegation_criteria = BamDelegationCriteria::new();

        Ok(Self {
            stake_pool,
            rpc_client,
            bam_api_client,
            bam_validators_store,
            bam_epoch_metric_store,
            bam_delegation_criteria,
        })
    }

    /// Run [`BamWriterService`]
    pub async fn run(&self) -> anyhow::Result<()> {
        let epoch_info = self.rpc_client.get_epoch_info().await?;
        let epoch = epoch_info.epoch;

        let jitosol_pool = get_stake_pool_account(&self.rpc_client, &self.stake_pool).await?;
        let jitosol_stake = jitosol_pool.total_lamports;

        let bam_node_validators = self.bam_api_client.get_validators().await?;

        let vote_accounts = self.rpc_client.get_vote_accounts().await?;
        let vote_account_by_node: HashMap<_, _> = vote_accounts
            .current
            .iter()
            .map(|v| (v.node_pubkey.clone(), v))
            .collect();

        let mut bam_validator_map = HashMap::new();
        for bam_node_validator in bam_node_validators {
            if let Some(vote_account) =
                vote_account_by_node.get(&bam_node_validator.validator_pubkey)
            {
                bam_validator_map
                    .insert(Pubkey::from_str(&vote_account.vote_pubkey)?, vote_account);
            }
        }

        let validator_histories =
            get_all_validator_history_accounts(&self.rpc_client.clone(), validator_history::id())
                .await?;

        let eligibility_checker = BamValidatorEligibility::new(epoch, &validator_histories);
        let mut bam_eligible_validators: Vec<BamValidator> = Vec::new();

        for validator_history in validator_histories.iter() {
            if let Some(vote_account) = bam_validator_map.get(&validator_history.vote_account) {
                match eligibility_checker.check_eligibility(validator_history) {
                    Ok(()) => {
                        let bam_validator = BamValidator::new(
                            vote_account.activated_stake,
                            epoch,
                            &vote_account.node_pubkey,
                            &vote_account.vote_pubkey,
                        );
                        bam_eligible_validators.push(bam_validator);
                    }
                    Err(reason) => {
                        log::debug!(
                            "Validator {} ineligible: {:?}",
                            vote_account.vote_pubkey,
                            reason
                        );
                    }
                }
            }
        }

        self.bam_validators_store
            .insert_many(&bam_eligible_validators)
            .await?;

        let total_stake = vote_accounts
            .current
            .iter()
            .map(|v| v.activated_stake)
            .sum();

        let eligible_bam_validator_count = bam_eligible_validators.len() as u64;
        let bam_stake = bam_eligible_validators
            .iter()
            .map(|v| v.get_active_stake())
            .sum();

        let mut current_epoch_metric = BamEpochMetric::new(
            epoch,
            bam_stake,
            total_stake,
            jitosol_stake,
            eligible_bam_validator_count,
        );

        let previous_epoch_metric = if let Some(prev_epoch) = epoch.checked_sub(1) {
            self.bam_epoch_metric_store
                .find_by_epoch(prev_epoch)
                .await?
        } else {
            None
        };

        let allocation_percentage = self
            .bam_delegation_criteria
            .calculate_current_allocation(&current_epoch_metric, previous_epoch_metric.as_ref());

        let available_delegation = self
            .bam_delegation_criteria
            .calculate_available_delegation(allocation_percentage, jitosol_stake);

        current_epoch_metric.set_allocation_bps(allocation_percentage);
        current_epoch_metric.set_available_bam_delegation_stake(available_delegation);

        self.bam_epoch_metric_store
            .upsert(current_epoch_metric)
            .await?;

        Ok(())
    }
}
