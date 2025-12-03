use std::{collections::HashMap, str::FromStr, sync::Arc};

use anyhow::anyhow;
use bam_api_client::{client::BamApiClient, types::ValidatorsResponse};
use clap::ValueEnum;
use kobe_core::db_models::{
    bam_epoch_metrics::{BamEpochMetrics, BamEpochMetricsStore},
    bam_validators::{BamValidator, BamValidatorStore},
};
use mongodb::Collection;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_pubkey::Pubkey;
use stakenet_sdk::{
    models::cluster::Cluster,
    utils::accounts::{get_all_validator_history_accounts, get_stake_pool_account},
};

use crate::{
    bam_delegation_criteria::BamDelegationCriteria,
    bam_validator_eligibility::{BamValidatorEligibility, IneligibilityReason},
};

mod bam_delegation_criteria;
mod bam_validator_eligibility;

pub struct BamWriterService {
    /// Cluster name (mainnet-beta, testnet)
    cluster: Cluster,

    /// Stake pool address
    stake_pool: Pubkey,

    /// RPC Client
    rpc_client: Arc<RpcClient>,

    /// BAM API base url
    bam_api_base_url: String,

    /// Bam validators store
    bam_validators_store: BamValidatorStore,

    /// Bam epoch metrics store
    bam_epoch_metrics_store: BamEpochMetricsStore,

    /// BAM Delegation Criteria
    bam_delegation_criteria: BamDelegationCriteria,
}

impl BamWriterService {
    /// Initialize [`BamWriterService`]
    pub async fn new(
        cluster: &str,
        mongo_connection_uri: &str,
        mongo_db_name: &str,
        stake_pool: Pubkey,
        rpc_client: Arc<RpcClient>,
        bam_api_base_url: &str,
    ) -> anyhow::Result<Self> {
        let cluster = Cluster::from_str(cluster, false)
            .map_err(|e| anyhow!("Failed to read cluster: {e}"))?;

        // Connect to MongoDB
        let client = mongodb::Client::with_uri_str(mongo_connection_uri).await?;
        let db = client.database(mongo_db_name);

        let bam_validators_collection: Collection<BamValidator> =
            db.collection(BamValidatorStore::COLLECTION);
        let bam_validators_store = BamValidatorStore::new(bam_validators_collection);

        let bam_epoch_metrics_collection: Collection<BamEpochMetrics> =
            db.collection(BamEpochMetricsStore::COLLECTION);
        let bam_epoch_metrics_store = BamEpochMetricsStore::new(bam_epoch_metrics_collection);

        let bam_delegation_criteria = BamDelegationCriteria::new();

        Ok(Self {
            cluster,
            stake_pool,
            rpc_client,
            bam_api_base_url: bam_api_base_url.to_string(),
            bam_validators_store,
            bam_epoch_metrics_store,
            bam_delegation_criteria,
        })
    }

    /// Retrieves the list of BAM (Block Auction Mechanism) validators based on the cluster configuration.
    ///
    /// # Behavior
    ///
    /// - **Localnet/Testnet**: Returns a static list of mock validators for testing purposes.
    ///   The mock data includes two validators with equal stake distribution (50% each).
    ///
    /// - **Mainnet**: Fetches the live validator list from the BAM API endpoint.
    async fn get_bam_validators(&self) -> anyhow::Result<Vec<ValidatorsResponse>> {
        match self.cluster {
            Cluster::Localnet | Cluster::Testnet => {
                let res = vec![
                    ValidatorsResponse {
                        validator_pubkey: "FT9QgTVo375TgDAQusTgpsfXqTosCJLfrBpoVdcbnhtS"
                            .to_string(),
                        bam_node_connection: "testnet-bam-1".to_string(),
                        stake: 1500000.0,
                        stake_percentage: 0.50,
                    },
                    ValidatorsResponse {
                        validator_pubkey: "141vSYKGRPNGieSrGJy8EeDVBcbjSr6aWkimNgrNZ6xN"
                            .to_string(),
                        bam_node_connection: "testnet-bam-1".to_string(),
                        stake: 1500000.0,
                        stake_percentage: 0.50,
                    },
                ];

                Ok(res)
            }
            Cluster::Mainnet => {
                let bam_api_config =
                    bam_api_client::config::Config::custom(self.bam_api_base_url.clone());
                let bam_api_client = BamApiClient::new(bam_api_config);

                bam_api_client
                    .get_validators()
                    .await
                    .map_err(|e| anyhow!("Failed to get bam validators: {e}"))
            }
        }
    }

    /// Run [`BamWriterService`]
    pub async fn run(&self) -> anyhow::Result<()> {
        let epoch_info = self.rpc_client.get_epoch_info().await?;
        let epoch = epoch_info.epoch;

        let jitosol_pool = get_stake_pool_account(&self.rpc_client, &self.stake_pool).await?;
        let jitosol_stake = jitosol_pool.total_lamports;

        let bam_node_validators = self.get_bam_validators().await?;

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
        let mut bam_validators: Vec<BamValidator> = Vec::new();

        for validator_history in validator_histories.iter() {
            if let Some(vote_account) = bam_validator_map.get(&validator_history.vote_account) {
                let vote_pubkey = &vote_account.vote_pubkey;
                let mut bam_validator = BamValidator::new(
                    vote_account.activated_stake,
                    epoch,
                    &vote_account.node_pubkey,
                    false,
                    vote_pubkey,
                );

                match eligibility_checker.check_eligibility(validator_history) {
                    Ok(()) => {
                        bam_validator.set_is_eligible(true);
                    }
                    Err(reason) => {
                        let reason_string = match reason {
                            IneligibilityReason::NotBamClient => "NotBamClient".to_string(),
                            IneligibilityReason::NonZeroCommission { epoch, commission } => {
                                format!("NonZeroCommission: {} in epoch {}", commission, epoch)
                            }
                            IneligibilityReason::HighMevCommission {
                                epoch,
                                mev_commission,
                            } => {
                                format!("HighMevCommission: {} in epoch {}", mev_commission, epoch)
                            }
                            IneligibilityReason::InSuperminority { epoch } => {
                                format!("InSuperminority in epoch {}", epoch)
                            }
                            IneligibilityReason::LowVoteCredits {
                                epoch,
                                credits,
                                min_required,
                            } => {
                                format!(
                                    "LowVoteCredits: {} credits (required: {}) in epoch {}",
                                    credits, min_required, epoch
                                )
                            }
                            IneligibilityReason::InsufficientHistory => {
                                "InsufficientHistory: Less than 3 epochs".to_string()
                            }
                        };
                        bam_validator.set_ineligibility_reason(Some(reason_string));
                    }
                }

                bam_validators.push(bam_validator);
            }
        }

        self.bam_validators_store
            .upsert(&bam_validators, epoch)
            .await?;

        let total_stake = vote_accounts
            .current
            .iter()
            .map(|v| v.activated_stake)
            .sum();

        let eligible_bam_validators = bam_validators
            .into_iter()
            .filter(|bv| bv.is_eligible())
            .collect::<Vec<BamValidator>>();
        let bam_stake = eligible_bam_validators
            .iter()
            .map(|v| v.get_active_stake())
            .sum();

        let mut current_epoch_metrics = BamEpochMetrics::new(
            epoch,
            bam_stake,
            total_stake,
            jitosol_stake,
            eligible_bam_validators.len() as u64,
        );

        let previous_epoch_metrics = if let Some(prev_epoch) = epoch.checked_sub(1) {
            self.bam_epoch_metrics_store
                .find_by_epoch(prev_epoch)
                .await?
        } else {
            None
        };

        let allocation_percentage = self
            .bam_delegation_criteria
            .calculate_current_allocation(&current_epoch_metrics, previous_epoch_metrics.as_ref());

        let available_delegation = self
            .bam_delegation_criteria
            .calculate_available_delegation(allocation_percentage, jitosol_stake);

        current_epoch_metrics.set_allocation_bps(allocation_percentage);
        current_epoch_metrics.set_available_bam_delegation_stake(available_delegation);

        self.bam_epoch_metrics_store
            .upsert(current_epoch_metrics)
            .await?;

        Ok(())
    }
}
