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

use crate::bam_delegation_criteria::BamDelegationCriteria;

mod bam_delegation_criteria;

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

        let jitosol_stake = get_stake_pool_account(&self.rpc_client, &self.stake_pool).await?;

        let bam_node_validators = self.bam_api_client.get_validators().await?;
        //  let bam_validator_map: HashMap<&str, &ValidatorsResponse> = bam_node_validators
        //      .iter()
        //      .map(|v| (v.validator_pubkey.as_str(), v))
        //      .collect();

        let vote_accounts = self.rpc_client.get_vote_accounts().await?;

        let mut bam_validator_map = HashMap::new();
        for bam_node_validator in bam_node_validators {
            for vote_account in vote_accounts.current.iter() {
                if vote_account
                    .node_pubkey
                    .eq(&bam_node_validator.validator_pubkey)
                {
                    bam_validator_map.insert(
                        Pubkey::from_str(&vote_account.vote_pubkey).unwrap(),
                        vote_account,
                    );
                }
            }
        }

        let validator_histories =
            get_all_validator_history_accounts(&self.rpc_client.clone(), jito_steward::id())
                .await?;

        let start_epoch = epoch - 3;
        let end_epoch = epoch;
        let mut bam_validators: Vec<BamValidator> = Vec::new();
        'validator_history: for validator_history in validator_histories {
            if let Some(vote_account) = bam_validator_map.get(&validator_history.vote_account) {
                for entry in validator_history
                    .history
                    .epoch_range(start_epoch as u16, end_epoch as u16)
                {
                    if let Some(entry) = entry {
                        if entry.commission.ne(&0) {
                            continue 'validator_history;
                        }

                        if entry.mev_commission.gt(&10) {
                            continue 'validator_history;
                        }

                        if entry.is_superminority.eq(&0) {
                            continue 'validator_history;
                        }
                    }
                }

                let bam_validator = BamValidator::new(
                    vote_account.activated_stake,
                    epoch,
                    &vote_account.node_pubkey,
                    &vote_account.vote_pubkey,
                );
                bam_validators.push(bam_validator);
            }
        }

        self.bam_validators_store
            .insert_many(&bam_validators)
            .await?;

        let total_stake = vote_accounts
            .current
            .iter()
            .map(|v| v.activated_stake)
            .sum();

        // let validators_url = format!("{}/api/v1/validators", self.kobe_base_api_url);

        // let client = reqwest::Client::new();
        // let validators = client
        //     .post(&validators_url)
        //     .json(&serde_json::json!({ "epoch": epoch }))
        //     .send()
        //     .await?
        //     .json::<kobe_api::schemas::validator::ValidatorsResponse>()
        //     .await?;

        let eligible_bam_validator_count = bam_validators.len() as u64;
        let bam_stake = bam_validators.iter().map(|v| v.get_active_stake()).sum();

        let available_bam_delegation_stake = self
            .bam_delegation_criteria
            .calculate_available_delegation(bam_stake, total_stake, jitosol_stake.total_lamports);

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
