use std::{collections::HashMap, str::FromStr, sync::Arc};

use anyhow::anyhow;
use bam_api_client::{client::BamApiClient, types::ValidatorsResponse};
use clap::ValueEnum;
use kobe_client::{client::KobeClient, client_builder::KobeApiClientBuilder};
use kobe_core::db_models::{
    bam_epoch_metrics::{BamEpochMetrics, BamEpochMetricsStore},
    bam_validators::{BamValidator, BamValidatorStore},
};
use mongodb::Collection;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_metrics::datapoint_info;
use solana_pubkey::Pubkey;
use stakenet_sdk::{
    models::cluster::Cluster,
    utils::accounts::{
        get_all_steward_accounts, get_all_validator_history_accounts, get_stake_pool_account,
    },
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

    /// Kobe api client
    kobe_api_client: KobeClient,

    /// Stake pool address
    stake_pool: Pubkey,

    /// Steward config
    steward_config: Pubkey,

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

    /// Override eligible validators with a hardcoded list (vote account pubkeys)
    override_eligible_validators: Option<Vec<Pubkey>>,

    /// Override available BAM delegation stake amount (in lamports)
    override_delegation_lamports: Option<u64>,
}

impl BamWriterService {
    /// Initialize [`BamWriterService`]
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        cluster: &str,
        mongo_connection_uri: &str,
        mongo_db_name: &str,
        stake_pool: Pubkey,
        steward_config: Pubkey,
        rpc_client: Arc<RpcClient>,
        bam_api_base_url: &str,
        kobe_api_base_url: &str,
        override_eligible_validators: Option<Vec<Pubkey>>,
        override_delegation_lamports: Option<u64>,
    ) -> anyhow::Result<Self> {
        let cluster = Cluster::from_str(cluster, false)
            .map_err(|e| anyhow!("Failed to read cluster: {e}"))?;

        let kobe_api_client = KobeApiClientBuilder::new()
            .base_url(kobe_api_base_url)
            .build();

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

        if let Some(ref overrides) = override_eligible_validators {
            log::info!(
                "Using override eligible validators ({} pubkeys): {:?}",
                overrides.len(),
                overrides
            );
        }

        if let Some(delegation) = override_delegation_lamports {
            log::info!(
                "Using override delegation amount: {} lamports ({:.2} SOL)",
                delegation,
                delegation as f64 / 1_000_000_000.0
            );
        }

        Ok(Self {
            cluster,
            kobe_api_client,
            stake_pool,
            steward_config,
            rpc_client,
            bam_api_base_url: bam_api_base_url.to_string(),
            bam_validators_store,
            bam_epoch_metrics_store,
            bam_delegation_criteria,
            override_eligible_validators,
            override_delegation_lamports,
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

        let vote_accounts = self.rpc_client.get_vote_accounts().await?;

        // Build vote account lookup by vote pubkey
        let vote_account_by_pubkey: HashMap<_, _> = vote_accounts
            .current
            .iter()
            .filter_map(|v| Pubkey::from_str(&v.vote_pubkey).ok().map(|pk| (pk, v)))
            .collect();

        let bam_validators: Vec<BamValidator> = if let Some(ref override_validators) =
            self.override_eligible_validators
        {
            // Override mode: use hardcoded list of vote account pubkeys
            log::info!(
                "Using override eligible validators ({} pubkeys)",
                override_validators.len()
            );

            let mut validators = Vec::new();
            for vote_pubkey in override_validators {
                if let Some(vote_account) = vote_account_by_pubkey.get(vote_pubkey) {
                    let mut bam_validator = BamValidator::new(
                        vote_account.activated_stake,
                        epoch,
                        &vote_account.node_pubkey,
                        true, // Mark as eligible
                        &vote_account.vote_pubkey,
                    );

                    datapoint_info!(
                        "bam-eligible-validators",
                        ("epoch", epoch, i64),
                        ("slot_index", epoch_info.slot_index, i64),
                        ("vote_pubkey", vote_pubkey.to_string(), String),
                        ("override", true, bool),
                        "cluster" => self.cluster.to_string(),
                    );

                    validators.push(bam_validator);
                } else {
                    log::warn!(
                        "Override validator {} not found in current vote accounts",
                        vote_pubkey
                    );
                }
            }
            validators
        } else {
            // Normal mode: query BAM API and check eligibility
            let bam_node_validators = self.get_bam_validators().await?;

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

            let bam_delegation_blacklist =
                self.kobe_api_client.get_bam_delegation_blacklist().await?;
            let blacklist_validators: Vec<Pubkey> = bam_delegation_blacklist
                .into_iter()
                .filter_map(|entry| Pubkey::from_str(&entry.vote_account).ok())
                .collect();

            let steward_all_accounts = get_all_steward_accounts(
                &self.rpc_client.clone(),
                &jito_steward::id(),
                &self.steward_config,
            )
            .await?;

            let validator_histories = get_all_validator_history_accounts(
                &self.rpc_client.clone(),
                validator_history::id(),
            )
            .await?;

            let eligibility_checker = BamValidatorEligibility::new(epoch, &validator_histories);
            let mut validators: Vec<BamValidator> = Vec::new();

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

                    match eligibility_checker.check_eligibility(
                        &blacklist_validators,
                        &steward_all_accounts.config_account,
                        validator_history,
                    ) {
                        Ok(()) => {
                            bam_validator.set_is_eligible(true);
                            datapoint_info!(
                                "bam-eligible-validators",
                                ("epoch", epoch, i64),
                                ("slot_index", epoch_info.slot_index, i64),
                                ("vote_pubkey", vote_pubkey.to_string(), String),
                                "cluster" => self.cluster.to_string(),
                            );
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
                                    format!(
                                        "HighMevCommission: {} in epoch {}",
                                        mev_commission, epoch
                                    )
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
                                IneligibilityReason::OnChainBlacklist => {
                                    format!("Blacklist on-chain in epoch {epoch}")
                                }
                                IneligibilityReason::OffChainBlacklist => {
                                    format!("Blacklist off-chain in epoch {epoch}")
                                }
                            };
                            bam_validator.set_ineligibility_reason(Some(reason_string));
                        }
                    }

                    validators.push(bam_validator);
                }
            }
            validators
        };

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

        let num_eligible_validators = eligible_bam_validators.len();

        let bam_stake = eligible_bam_validators
            .iter()
            .map(|v| v.get_active_stake())
            .sum();

        datapoint_info!(
            "bam-writer-run",
            ("epoch", epoch, i64),
            ("slot_index", epoch_info.slot_index, i64),
            ("bam_validator_count", num_eligible_validators as i64, i64),
            ("bam_stake", bam_stake as i64, i64),
            ("total_stake", total_stake as i64, i64),
            "cluster" => self.cluster.to_string(),
        );

        let mut current_epoch_metrics = BamEpochMetrics::new(
            epoch,
            bam_stake,
            total_stake,
            jitosol_stake,
            eligible_bam_validators.len() as u64,
        );

        let (allocation_percentage, available_delegation) = if let Some(override_delegation) =
            self.override_delegation_lamports
        {
            // Override mode: use hardcoded delegation amount
            log::info!(
                "Using override delegation: {} lamports ({:.2} SOL)",
                override_delegation,
                override_delegation as f64 / 1_000_000_000.0
            );

            (
                0u64,
                override_delegation * eligible_bam_validators.len() as u64,
            )
        } else {
            // Normal mode: calculate allocation based on criteria
            let previous_epoch_metrics = if let Some(prev_epoch) = epoch.checked_sub(1) {
                self.bam_epoch_metrics_store
                    .find_by_epoch(prev_epoch)
                    .await?
            } else {
                None
            };

            let allocation_percentage = self.bam_delegation_criteria.calculate_current_allocation(
                &current_epoch_metrics,
                previous_epoch_metrics.as_ref(),
            );

            let available_delegation = self
                .bam_delegation_criteria
                .calculate_available_delegation(allocation_percentage, jitosol_stake);

            (allocation_percentage, available_delegation)
        };

        current_epoch_metrics.set_allocation_bps(allocation_percentage);
        current_epoch_metrics.set_available_bam_delegation_stake(available_delegation);

        let delegation_per_validator = self
            .override_delegation_lamports
            .unwrap_or(available_delegation / eligible_bam_validators.len() as u64);

        datapoint_info!(
            "bam-writer-run",
            ("epoch", epoch, i64),
            ("slot_index", epoch_info.slot_index, i64),
            ("allocation_bps", allocation_percentage, i64),
            ("available_delegation", available_delegation, i64),
            ("delegation_per_validator", delegation_per_validator, i64),
            "cluster" => self.cluster.to_string(),
        );

        self.bam_epoch_metrics_store
            .upsert(current_epoch_metrics)
            .await?;

        Ok(())
    }
}
