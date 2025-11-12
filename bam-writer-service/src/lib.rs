use std::{collections::HashMap, sync::Arc, time::Duration};

use bam_api_client::{client::BamApiClient, types::ValidatorsResponse};
use kobe_core::db_models::bam_epoch_metric::{BamEpochMetric, BamEpochMetricStore};
use mongodb::Collection;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
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

    /// Kobe API base URL
    kobe_base_api_url: String,

    /// Bam epoch metric store
    bam_epoch_metric_store: BamEpochMetricStore,
}

impl BamWriterService {
    /// Initialize [`BamWriterService`]
    pub async fn new(
        mongo_connection_uri: &str,
        mongo_db_name: &str,
        stake_pool: Pubkey,
        rpc_url: &str,
        bam_api_base_url: &str,
        kobe_api_base_url: &str,
    ) -> anyhow::Result<Self> {
        // Connect to MongoDB
        let client = mongodb::Client::with_uri_str(mongo_connection_uri).await?;
        let db = client.database(mongo_db_name);

        let bam_epoch_metric_collection: Collection<BamEpochMetric> =
            db.collection(BamEpochMetricStore::COLLECTION);
        let bam_epoch_metric_store = BamEpochMetricStore::new(bam_epoch_metric_collection);

        // Connect to RPC node
        let rpc_client = RpcClient::new_with_timeout_and_commitment(
            rpc_url.to_string(),
            Duration::from_secs(20),
            CommitmentConfig::finalized(),
        );

        let bam_api_config = bam_api_client::config::Config::custom(bam_api_base_url);
        let bam_api_client = BamApiClient::new(bam_api_config);

        Ok(Self {
            stake_pool,
            rpc_client: Arc::new(rpc_client),
            bam_api_client,
            kobe_base_api_url: kobe_api_base_url.to_string(),
            bam_epoch_metric_store,
        })
    }

    /// Run [`BamWriterService`]
    pub async fn run(&self) -> anyhow::Result<()> {
        let epoch_info = self.rpc_client.get_epoch_info().await?;
        let epoch = epoch_info.epoch;

        let jitosol_stake = get_stake_pool_account(&self.rpc_client, &self.stake_pool).await?;

        let bam_node_validators = self.bam_api_client.get_validators().await?;
        let bam_validator_map: HashMap<&str, &ValidatorsResponse> = bam_node_validators
            .iter()
            .map(|v| (v.validator_pubkey.as_str(), v))
            .collect();

        let vote_accounts = self.rpc_client.get_vote_accounts().await?;
        let total_stake = vote_accounts
            .current
            .iter()
            .map(|v| v.activated_stake)
            .sum();

        let validators_url = format!(
            "{}/api/v1/validators?epoch={}",
            self.kobe_base_api_url, epoch
        );
        let validators = reqwest::get(&validators_url)
            .await?
            .json::<kobe_api::schemas::validator::ValidatorsResponse>()
            .await?;

        let mut eligible_bam_validator_count = 0_u64;
        let mut bam_stake = 0_u64;
        for validator in validators.validators.iter() {
            if let Some(ref identity_account) = validator.identity_account {
                if bam_validator_map.contains_key(identity_account.as_str()) {
                    bam_stake += validator.active_stake;

                    if let Some(true) = validator.jito_pool_eligible {
                        eligible_bam_validator_count += 1;
                    }
                }
            }
        }

        let criteria = BamDelegationCriteria::new();
        let available_bam_delegation_stake = criteria.calculate_available_delegation(
            bam_stake,
            total_stake,
            jitosol_stake.total_lamports,
        );

        let bam_epoch_metric = BamEpochMetric::new(
            epoch,
            bam_stake,
            available_bam_delegation_stake,
            eligible_bam_validator_count,
        );

        self.bam_epoch_metric_store.upsert(bam_epoch_metric).await?;

        Ok(())
    }
}
