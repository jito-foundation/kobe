use std::{collections::HashMap, sync::Arc};

use bam_api_client::{client::BamApiClient, types::ValidatorsResponse};
use kobe_api_client::{client::KobeApiClient, config::Config};
use kobe_core::{
    db_models::bam_epoch_metric::{BamEpochMetric, BamEpochMetricStore},
    validators_app::Cluster,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_pubkey::Pubkey;
use stakenet_sdk::utils::accounts::get_stake_pool_account;

use crate::bam_delegation_criteria::BamDelegationCriteria;

mod bam_delegation_criteria;

pub struct BamWriterService {
    /// Stake pool address
    stake_pool: Pubkey,

    /// RPC Client
    rpc_client: Arc<RpcClient>,

    /// BAM API client
    bam_api_client: BamApiClient,

    /// Bam epoch metric store
    bam_epoch_metric_store: BamEpochMetricStore,

    /// Cluster
    cluster: Cluster,
}

impl BamWriterService {
    /// Initialize [`BamWriterService`]
    pub fn new(
        stake_pool: Pubkey,
        rpc_client: RpcClient,
        bam_api_client: BamApiClient,
        bam_epoch_metric_store: BamEpochMetricStore,
        cluster: Cluster,
    ) -> Self {
        Self {
            stake_pool,
            rpc_client: Arc::new(rpc_client),
            bam_api_client,
            bam_epoch_metric_store,
            cluster,
        }
    }

    /// Run [`BamWriterService`]
    pub async fn run(&self) -> anyhow::Result<()> {
        let epoch_info = self.rpc_client.get_epoch_info().await?;
        let epoch = epoch_info.epoch;

        let jitosol_stake =
            get_stake_pool_account(&self.rpc_client.clone(), &self.stake_pool).await?;

        let bam_node_validators = self.bam_api_client.get_validators().await?;
        let bam_validator_map: HashMap<String, &ValidatorsResponse> = bam_node_validators
            .iter()
            .map(|v| (v.validator_pubkey.clone(), v))
            .collect();

        let vote_accounts = self.rpc_client.get_vote_accounts().await?;
        let total_sol_stake = vote_accounts
            .current
            .iter()
            .map(|v| v.activated_stake)
            .sum();

        let kobe_api_config = match self.cluster {
            Cluster::MainnetBeta => Config::mainnet(),
            Cluster::Testnet | Cluster::Devnet => Config::testnet(),
        };
        let kobe_api_client = KobeApiClient::new(kobe_api_config);

        let validators = kobe_api_client.get_validators(Some(epoch)).await?;

        let mut eligible_bam_validator_count = 0_u64;
        let mut bam_sol_stake = 0_u64;
        for validator in validators.validators.iter() {
            if let Some(ref identity_account) = validator.identity_account {
                if bam_validator_map.contains_key(identity_account) {
                    bam_sol_stake += validator.active_stake;

                    if let Some(true) = validator.jito_pool_eligible {
                        eligible_bam_validator_count += 0;
                    }
                }
            }
        }

        let bam_total_network_stake_weight: f64 = bam_node_validators.iter().map(|v| v.stake).sum();

        let criteria = BamDelegationCriteria::new();
        let available_bam_delegation_stake = criteria.calculate_available_delegation(
            bam_sol_stake,
            total_sol_stake,
            jitosol_stake.total_lamports,
        );

        let bam_epoch_metric = BamEpochMetric::new(
            epoch,
            bam_total_network_stake_weight,
            available_bam_delegation_stake,
            eligible_bam_validator_count,
        );

        self.bam_epoch_metric_store.insert(bam_epoch_metric).await?;

        Ok(())
    }
}
