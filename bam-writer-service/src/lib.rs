use std::{collections::HashMap, sync::Arc};

use bam_api_client::client::BamApiClient;
use kobe_core::db_models::bam_epoch_metric::{BamEpochMetric, BamEpochMetricStore};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_pubkey::Pubkey;
use stakenet_sdk::utils::accounts::{
    get_all_validator_history_accounts, get_steward_config_account,
};

mod bam_delegation_criteria;

pub struct BamWriterService {
    /// Validator history accounts
    validator_history_program_id: Pubkey,

    /// Steward config pubkey
    steward_config: Pubkey,

    /// Stake pool address
    stake_pool: Pubkey,

    /// RPC Client
    rpc_client: Arc<RpcClient>,

    /// BAM API client
    bam_api_client: BamApiClient,

    /// Bam epoch metric store
    bam_epoch_metric_store: BamEpochMetricStore,
}

impl BamWriterService {
    /// Initialize [`BamWriterService`]
    pub fn new(
        validator_history_program_id: Pubkey,
        steward_config: Pubkey,
        stake_pool: Pubkey,
        rpc_client: RpcClient,
        bam_api_client: BamApiClient,
        bam_epoch_metric_store: BamEpochMetricStore,
    ) -> Self {
        Self {
            validator_history_program_id,
            steward_config,
            stake_pool,
            rpc_client: Arc::new(rpc_client),
            bam_api_client,
            bam_epoch_metric_store,
        }
    }

    /// Run [`BamWriterService`]
    pub async fn run(&self) -> anyhow::Result<()> {
        let epoch_info = self.rpc_client.get_epoch_info().await?;
        let epoch = epoch_info.epoch;

        // let stake_pool = get_stake_pool_account(&self.rpc_client.clone(), &self.stake_pool).await?;
        // let validator_list =
        //     get_validator_list_account(&self.rpc_client.clone(), &stake_pool.validator_list)
        //         .await?;

        // for validator in validator_list.find()

        let bam_node_validators = self.bam_api_client.get_validators().await?;
        // let bam_validators: Vec<BamValidator> = bam_node_validators
        //     .iter()
        //     .map(|v| BamValidator::from_bam_api(epoch, v.validator_pubkey))
        //     .collect();

        // let bam_validator_map = HashMap::new();
        let vote_accounts = self.rpc_client.get_vote_accounts().await?;
        // for bam_validator in bam_validators.iter() {
        //     for current_vote_account in vote_accounts.current.iter() {
        //         if bam_validator
        //             .get_identity()
        //             .eq(&current_vote_account.node_pubkey)
        //         {
        //             bam_validator.set_vote_account(current_vote_account.vote_pubkey);
        //             bam_validator_map
        //                 .entry(current_vote_account.vote_pubkey)
        //                 .insert_entry(bam_validator);
        //         }
        //     }
        // }

        let validator_histories = get_all_validator_history_accounts(
            &self.rpc_client.clone(),
            self.validator_history_program_id,
        )
        .await?;

        let config =
            get_steward_config_account(&self.rpc_client.clone(), &self.steward_config).await?;

        let start_epoch =
            epoch.saturating_sub(config.parameters.minimum_voting_epochs.saturating_sub(1));
        // for validator_history in validator_histories {
        //     if let Some(entry) = validator_history.history.last() {
        //         // Steward requires that validators have been active for last minimum_voting_epochs epochs
        //         if validator_history
        //             .history
        //             .epoch_credits_range(start_epoch as u16, epoch as u16)
        //             .iter()
        //             .any(|entry| entry.is_none())
        //         {
        //             continue;
        //         }
        //         if entry
        //             .activated_stake_lamports
        //             .eq(&ValidatorHistoryEntry::default().activated_stake_lamports)
        //         {
        //             continue;
        //         }
        //         if entry.activated_stake_lamports < config.parameters.minimum_stake_lamports {
        //             continue;
        //         }

        //         bam_validator_map
        //             .entry(validator_history.vote_account.to_string())
        //             .and_modify(|bam_validator| bam_validator.set_eligible());
        //     }
        // }

        let bam_total_network_stake_weight: f64 = bam_node_validators.iter().map(|v| v.stake).sum();
        // let eligible_bam_validator_count = bam_node_validators.iter().len();

        // let delinquent_vote_accounts: HashSet<String> = HashSet::from_iter(
        //     vote_accounts
        //         .delinquent
        //         .iter()
        //         .map(|v| v.node_pubkey.clone())
        //         .collect::<Vec<String>>(),
        // );

        // let mut bam_validators = Vec::new();
        // for bam_validator in bam_node_validators {
        //     for vote_account in vote_accounts.current.iter() {
        //         if bam_validator.validator_pubkey.eq(&vote_account.node_pubkey) {
        //             let bam_validator = BamValidator::new(
        //                 epoch,
        //                 vote_account.vote_pubkey.clone(),
        //                 0, // FIXME
        //                 !delinquent_vote_accounts.contains(&vote_account.node_pubkey),
        //                 false, // FIXME
        //             );
        //             bam_validators.push(bam_validator);
        //         }
        //     }
        // }

        // let bam_epoch_metric = BamEpochMetric::new(
        //     epoch,
        //     bam_total_network_stake_weight,
        //     0, // FIXME
        //     eligible_bam_validator_count as u64,
        // );

        // self.bam_epoch_metric_store.insert(bam_epoch_metric).await?;

        Ok(())
    }
}
