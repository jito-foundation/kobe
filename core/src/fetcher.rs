use std::{collections::HashMap, str::FromStr, sync::Arc};

use anchor_lang::AccountDeserialize;
use bincode::deserialize;
use jito_priority_fee_distribution::state::PriorityFeeDistributionAccount;
use jito_priority_fee_distribution_sdk::derive_priority_fee_distribution_account_address;
use jito_steward::state::Config as StewardConfig;
use jito_tip_distribution::state::TipDistributionAccount;
use log::warn;
use solana_account::Account;
use solana_account_decoder::{
    parse_config::{parse_config, ConfigAccountType},
    validator_info,
};
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_response::RpcVoteAccountStatus};
use solana_config_interface::state::ConfigKeys;
use solana_pubkey::Pubkey;
use spl_stake_pool::state::ValidatorStakeInfo;
use spl_stake_pool_cli::client::get_validator_list;
use stakenet_sdk::utils::accounts::{
    get_all_validator_history_accounts, get_steward_config_account,
};
use validator_history::ValidatorHistory;

use crate::{
    client_type::ClientType,
    constants::{
        PRIORITY_FEE_DISTRIBUTION_PROGRAM, STEWARD_CONFIG_MAINNET, STEWARD_CONFIG_TESTNET,
        TIP_DISTRIBUTION_PROGRAM_MAINNET, TIP_DISTRIBUTION_PROGRAM_TESTNET,
        VALIDATOR_HISTORY_PROGRAM_MAINNET, VALIDATOR_HISTORY_PROGRAM_TESTNET,
    },
    validators_app::Cluster,
};

type Error = Box<dyn std::error::Error>;

#[derive(Default, Debug)]
pub struct ValidatorChainData {
    /// Validator display name
    pub name: Option<String>,

    /// Validator website URL
    pub website: Option<String>,

    /// MEV commission rate in basis points (100 = 1%)
    pub mev_commission_bps: Option<u16>,

    /// Total MEV rewards earned this epoch in lamports
    pub mev_revenue_lamports: u64,

    /// Whether validator is running Jito client
    pub running_jito: bool,

    /// Validator's vote credits relative to network average
    pub vote_credit_proportion: f64,

    /// Stake pool validator info
    pub stake_info: Option<ValidatorStakeInfo>,

    /// Total lamports staked across all validators
    pub total_staked_lamports: u64,

    /// Inflation rewards earned this epoch in lamports
    pub inflation_rewards_lamports: u64,

    /// Priority fee commission rate in basis points
    pub priority_fee_commission_bps: u16,

    /// Total priority fee rewards earned this epoch in lamports
    pub priority_fee_revenue_lamports: u64,

    /// Is jito blacklist
    pub is_jito_blacklist: Option<bool>,
}

/// Unified validator data fetcher that handles all on-chain data collection
///
/// Fetches MEV rewards, priority fees, vote credits, staking info, and validator metadata directly
/// from on-chain data
pub struct ValidatorDataFetcher {
    /// RPC Client
    rpc_client: Arc<RpcClient>,

    /// Network Cluster (mainnet-beta/testnet)
    cluster: Cluster,

    /// Stake pool's validator list pubkey
    validator_list_pubkey: Pubkey,
}

impl ValidatorDataFetcher {
    /// Initialize [`ValidatorDataFetcher`]
    pub fn new(
        rpc_client: Arc<RpcClient>,
        cluster: Cluster,
        validator_list_pubkey: Pubkey,
    ) -> Self {
        Self {
            rpc_client,
            cluster,
            validator_list_pubkey,
        }
    }

    /// Fetches on-chain data for all validators
    ///
    /// Automatically discovers validators from vote accounts and aggregates:
    /// - MEV rewards and commission rates from tip distribution accounts
    /// - Priority fee rewards and commission rates
    /// - Vote credits and performance metrics
    /// - Staking information and inflation rewards
    /// - Validator metadata (name, website) from config accounts
    /// - Jito client detection from tip accounts and validator history
    pub async fn fetch_chain_data(
        &self,
        epoch: u64,
    ) -> Result<HashMap<Pubkey, ValidatorChainData>, Error> {
        let vote_accounts = self.rpc_client.get_vote_accounts().await?;

        // Fetch all base data in parallel
        let (inflation_rate, validator_histories, staked_validators) = tokio::try_join!(
            self.fetch_inflation_rate(),
            self.fetch_validator_history_accounts(),
            get_validator_list(&self.rpc_client, &self.validator_list_pubkey)
        )?;

        // Get all vote account pubkeys from the network
        let validator_vote_pubkeys: Vec<Pubkey> = vote_accounts
            .current
            .iter()
            .chain(vote_accounts.delinquent.iter())
            .filter_map(|acc| Pubkey::from_str(&acc.vote_pubkey).ok())
            .collect();

        // Fetch program-specific data in parallel
        let (tip_distributions, priority_fee_distributions, validator_info_map) = tokio::try_join!(
            self.fetch_tip_distribution_accounts(&validator_vote_pubkeys, epoch),
            self.fetch_priority_fee_distribution_accounts(&validator_vote_pubkeys, epoch),
            self.fetch_validator_info_map(&vote_accounts)
        )?;

        let (global_average, vote_credits_map) = self.calculate_vote_credits(&vote_accounts)?;
        let total_staked_lamports = self.calculate_total_stake(&vote_accounts);

        let steward_config_pubkey = self.get_steward_config_pubkey();
        let steward_config =
            get_steward_config_account(&self.rpc_client, &steward_config_pubkey).await?;

        // Build complete validator data
        let mut result = HashMap::new();
        for vote_account in validator_vote_pubkeys {
            let validator_data = self.build_single_validator_data(
                &vote_account,
                epoch,
                &tip_distributions,
                &priority_fee_distributions,
                &validator_histories,
                &vote_credits_map,
                global_average,
                &staked_validators,
                &vote_accounts,
                inflation_rate,
                total_staked_lamports,
                &validator_info_map,
                &steward_config,
            );
            result.insert(vote_account, validator_data);
        }

        Ok(result)
    }

    /// Build data for a single validator
    ///
    /// ## MEV Data
    ///
    /// - **Tip distribution account**: MEV rewards distributed to validators
    /// - **Priority fee distribution account**: Priority fees collected by validators
    /// - **Commission rates**: Validator commission on MEV rewards and priority revenue
    ///
    /// ## Staking & Performance
    ///
    /// - **Vote accounts**: Current validator vote account information
    /// - **Vote credits**: Performance metrics based on voting behavior
    /// - **Staking amounts**: Total lamports staked with each validator
    /// - **Inflation rewards**: Calculated based on stake and performance
    ///
    /// ## Jito Client Detection
    ///
    /// 1. **Tip account method**: Checks if validator has tip distribution account
    /// 2. **Validator-History method**: Checks validator history for Jito client type
    /// 3. **Combined detection**: Detect `running_jito` = (`has_tip_account || is_jito_client`)
    #[allow(clippy::too_many_arguments)]
    fn build_single_validator_data(
        &self,
        vote_account: &Pubkey,
        epoch: u64,
        tip_distributions: &HashMap<Pubkey, TipDistributionAccount>,
        priority_fee_distributions: &HashMap<Pubkey, PriorityFeeDistributionAccount>,
        validator_histories: &HashMap<Pubkey, ValidatorHistory>,
        vote_credits_map: &HashMap<Pubkey, f64>,
        global_average: f64,
        staked_validators: &spl_stake_pool::state::ValidatorList,
        vote_accounts: &RpcVoteAccountStatus,
        inflation_rate: f64,
        total_staked_lamports: u64,
        validator_info_map: &HashMap<Pubkey, (Option<String>, Option<String>)>,
        steward_config: &StewardConfig,
    ) -> ValidatorChainData {
        let validator_index = validator_histories
            .get(vote_account)
            .map(|history| history.index);

        // MEV data
        let (mev_commission_bps, mev_revenue_lamports) = tip_distributions
            .get(vote_account)
            .map(|tda| {
                let revenue = tda
                    .merkle_root
                    .as_ref()
                    .map(|mr| mr.max_total_claim)
                    .unwrap_or(0);
                (Some(tda.validator_commission_bps), revenue)
            })
            .unwrap_or((None, 0));

        // Priority fee data
        let (priority_fee_commission_bps, priority_fee_revenue_lamports) =
            priority_fee_distributions
                .get(vote_account)
                .map(|pfda| {
                    let revenue = pfda
                        .merkle_root
                        .as_ref()
                        .map(|mr| mr.max_total_claim)
                        .unwrap_or(0);
                    (pfda.validator_commission_bps, revenue)
                })
                .unwrap_or((10000, 0));

        // Jito detection
        let has_tip_account = tip_distributions.contains_key(vote_account);
        let is_jito_client = validator_histories
            .get(vote_account)
            .and_then(|history| {
                history
                    .history
                    .arr
                    .iter()
                    .find(|entry| entry.epoch == epoch as u16)
                    .map(|entry| {
                        matches!(ClientType::from_u8(entry.client_type), ClientType::JitoLabs)
                    })
            })
            .unwrap_or(false);
        let running_jito = has_tip_account || is_jito_client;

        // Performance metrics
        let vote_credits = vote_credits_map.get(vote_account).copied().unwrap_or(0.0);
        let vote_credit_proportion = if global_average > 0.0 {
            vote_credits / global_average
        } else {
            0.0
        };

        // Stake info
        let stake_info = staked_validators
            .validators
            .iter()
            .find(|info| info.vote_account_address == *vote_account)
            .cloned();

        // Inflation rewards
        let staked_amount = self.get_validator_stake(vote_accounts, vote_account) as f64;
        let inflation_rewards_lamports =
            self.calculate_inflation_rewards(inflation_rate, staked_amount, vote_credit_proportion);

        // Validator info (name, website)
        let (name, website) = validator_info_map
            .get(vote_account)
            .cloned()
            .unwrap_or((None, None));

        let is_jito_blacklist = validator_index.and_then(|index| {
            steward_config
                .validator_history_blacklist
                .get(index as usize)
                .ok()
        });

        ValidatorChainData {
            name,
            website,
            mev_commission_bps,
            mev_revenue_lamports,
            running_jito,
            vote_credit_proportion,
            stake_info,
            total_staked_lamports,
            inflation_rewards_lamports,
            priority_fee_commission_bps,
            priority_fee_revenue_lamports,
            is_jito_blacklist,
        }
    }

    /// Fetch validator info (name, website) from on-chain ValidatorInfo accounts
    ///
    /// Maps node pubkeys from config accounts to vote account pubkeys,
    /// extracting validator metadata like display names and websites.
    async fn fetch_validator_info_map(
        &self,
        vote_accounts: &RpcVoteAccountStatus,
    ) -> Result<HashMap<Pubkey, (Option<String>, Option<String>)>, Error> {
        let mut info_map = HashMap::new();

        let all_config = self
            .rpc_client
            .get_program_accounts(&solana_config_interface::id())
            .await?;

        let validator_info: Vec<(Pubkey, Account)> = all_config
            .into_iter()
            .filter(|(_, account)| {
                deserialize::<ConfigKeys>(&account.data)
                    .map(|keys| keys.keys.contains(&(validator_info::id(), false)))
                    .unwrap_or(false)
            })
            .collect();

        for (pubkey, info) in validator_info {
            if let Ok(ConfigAccountType::ValidatorInfo(value)) = parse_config(&info.data, &pubkey) {
                if let Some(key) = value.keys.get(1) {
                    // Find corresponding vote account
                    for vote_account in vote_accounts.current.iter() {
                        if vote_account.node_pubkey.eq(&key.pubkey) {
                            let vote_pubkey = Pubkey::from_str(&vote_account.vote_pubkey)?;
                            let name = value
                                .config_data
                                .get("name")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            let website = value
                                .config_data
                                .get("website")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            info_map.insert(vote_pubkey, (name, website));
                        }
                    }
                }
            }
        }

        Ok(info_map)
    }

    /// Fetch Jito tip distribution accounts for MEV reward tracking
    ///
    /// Retrieves tip distribution accounts for all validators to determine:
    /// - Which validators are running Jito client
    /// - MEV commission rates and revenue amounts
    async fn fetch_tip_distribution_accounts(
        &self,
        validator_vote_pubkeys: &[Pubkey],
        epoch: u64,
    ) -> Result<HashMap<Pubkey, TipDistributionAccount>, Error> {
        let mut commission_map = HashMap::new();
        for chunk in validator_vote_pubkeys.chunks(100) {
            let pubkeys = chunk
                .iter()
                .map(|vote_pubkey| {
                    jito_tip_distribution_sdk::derive_tip_distribution_account_address(
                        &self.get_tip_distribution_program_id(),
                        vote_pubkey,
                        epoch,
                    )
                    .0
                })
                .collect::<Vec<Pubkey>>();

            if let Ok(accounts) = self.rpc_client.get_multiple_accounts(&pubkeys).await {
                for (vote_pubkey, account) in chunk.iter().zip(accounts.iter()) {
                    if let Some(acc) = account {
                        if self.get_tip_distribution_program_id() != acc.owner {
                            warn!("Validator {} may be trying to mess with their Tip Distribution Account", vote_pubkey);
                            continue;
                        }
                        if let Ok(tip_distribution) =
                            TipDistributionAccount::try_deserialize(&mut acc.data.as_slice())
                        {
                            commission_map.insert(*vote_pubkey, tip_distribution);
                        }
                    }
                }
            }
        }
        Ok(commission_map)
    }

    /// Fetch priority fee distribution accounts
    ///
    /// Retrieves priority fee distribution accounts to determine commission rates
    /// and revenue from priority fees collected by validators.
    async fn fetch_priority_fee_distribution_accounts(
        &self,
        validator_vote_pubkeys: &[Pubkey],
        epoch: u64,
    ) -> Result<HashMap<Pubkey, PriorityFeeDistributionAccount>, Error> {
        let mut commission_map = HashMap::new();
        for chunk in validator_vote_pubkeys.chunks(100) {
            let pubkeys = chunk
                .iter()
                .map(|vote_pubkey| {
                    let vote_account_pubkey =
                        solana_pubkey::Pubkey::from_str(&vote_pubkey.to_string()).unwrap();
                    let pfda_pubkey = derive_priority_fee_distribution_account_address(
                        &self.get_priority_fee_distribution_program_id(),
                        &vote_account_pubkey,
                        epoch,
                    )
                    .0;
                    Pubkey::from_str(&pfda_pubkey.to_string()).unwrap()
                })
                .collect::<Vec<Pubkey>>();

            if let Ok(accounts) = self.rpc_client.get_multiple_accounts(&pubkeys).await {
                for (vote_pubkey, account) in chunk.iter().zip(accounts.iter()) {
                    if let Some(acc) = account {
                        let program_pubkey = Pubkey::from_str(
                            &self.get_priority_fee_distribution_program_id().to_string(),
                        )
                        .unwrap();
                        if program_pubkey != acc.owner {
                            warn!("Validator {} may be trying to mess with their Priority Fee Distribution Account", vote_pubkey);
                            continue;
                        }
                        if let Ok(fee_distribution) =
                            PriorityFeeDistributionAccount::try_deserialize(
                                &mut acc.data.as_slice(),
                            )
                        {
                            commission_map.insert(*vote_pubkey, fee_distribution);
                        }
                    }
                }
            }
        }
        Ok(commission_map)
    }

    /// Fetch all validator history accounts for client type detection
    ///
    /// Retrieves validator history to determine which client software validators
    /// are running (Jito, Solana Labs, etc.) for the current epoch.
    async fn fetch_validator_history_accounts(
        &self,
    ) -> Result<HashMap<Pubkey, ValidatorHistory>, Error> {
        let validator_history_program_id = self.get_validator_history_program_id();
        let validator_histories =
            get_all_validator_history_accounts(&self.rpc_client, validator_history_program_id)
                .await?;

        Ok(validator_histories
            .into_iter()
            .map(|h| (h.vote_account, h))
            .collect())
    }

    /// Calculate total MEV rewards distributed across all validators for an epoch
    ///
    /// Sums up the lamports in all tip distribution accounts, subtracting
    /// the rent exemption amount to get actual reward amounts.
    pub async fn fetch_mev_rewards(&self, epoch: u64) -> Result<u64, Error> {
        let vote_accounts = self.rpc_client.get_vote_accounts().await?;
        let validator_vote_pubkeys: Vec<Pubkey> = vote_accounts
            .current
            .iter()
            .chain(vote_accounts.delinquent.iter())
            .filter_map(|account| Pubkey::from_str(&account.vote_pubkey).ok())
            .collect();

        let mut total = 0;
        let tip_distribution_account_rent = self
            .rpc_client
            .get_minimum_balance_for_rent_exemption(TipDistributionAccount::SIZE)
            .await?;

        for chunk in validator_vote_pubkeys.chunks(100) {
            let pubkeys = chunk
                .iter()
                .map(|vote_pubkey| {
                    jito_tip_distribution_sdk::derive_tip_distribution_account_address(
                        &self.get_tip_distribution_program_id(),
                        vote_pubkey,
                        epoch,
                    )
                    .0
                })
                .collect::<Vec<Pubkey>>();

            if let Ok(accounts) = self.rpc_client.get_multiple_accounts(&pubkeys).await {
                for account in accounts.into_iter().flatten() {
                    if self.get_tip_distribution_program_id() == account.owner {
                        total += account.lamports - tip_distribution_account_rent;
                    }
                }
            }
        }
        Ok(total)
    }

    /// Get current network inflation rate
    async fn fetch_inflation_rate(&self) -> Result<f64, Error> {
        Ok(self.rpc_client.get_inflation_rate().await?.total)
    }

    /// Calculate vote credit performance metrics for all validators
    ///
    /// Computes average vote credits per validator over recent epochs and calculates
    /// the global network average for performance comparison.
    fn calculate_vote_credits(
        &self,
        vote_accounts: &RpcVoteAccountStatus,
    ) -> Result<(f64, HashMap<Pubkey, f64>), Error> {
        let mut vote_credits_map = HashMap::new();

        for vote_account in vote_accounts
            .current
            .iter()
            .chain(vote_accounts.delinquent.iter())
        {
            let pubkey = Pubkey::from_str(&vote_account.vote_pubkey)?;
            let sum: u64 = vote_account
                .epoch_credits
                .iter()
                .map(|(_, current, prev)| current - prev)
                .sum();
            let average = if vote_account.epoch_credits.is_empty() {
                0.0
            } else {
                sum as f64 / vote_account.epoch_credits.len() as f64
            };
            vote_credits_map.insert(pubkey, average);
        }

        let global_average = if vote_credits_map.is_empty() {
            0.0
        } else {
            vote_credits_map.values().sum::<f64>() / vote_credits_map.len() as f64
        };

        Ok((global_average, vote_credits_map))
    }

    /// Calculate total stake across all validators in the network
    pub fn calculate_total_stake(&self, vote_accounts: &RpcVoteAccountStatus) -> u64 {
        vote_accounts
            .current
            .iter()
            .chain(vote_accounts.delinquent.iter())
            .map(|info| info.activated_stake)
            .sum()
    }

    /// Get stake amount for a specific validator
    fn get_validator_stake(
        &self,
        vote_accounts: &RpcVoteAccountStatus,
        vote_account: &Pubkey,
    ) -> u64 {
        vote_accounts
            .current
            .iter()
            .chain(vote_accounts.delinquent.iter())
            .find(|info| Pubkey::from_str(&info.vote_pubkey).unwrap() == *vote_account)
            .map(|info| info.activated_stake)
            .unwrap_or(0)
    }

    /// Calculate inflation rewards for a validator based on stake and performance
    fn calculate_inflation_rewards(
        &self,
        inflation_rate: f64,
        staked_amount: f64,
        vote_credit_proportion: f64,
    ) -> u64 {
        const EPOCHS_PER_YEAR: f64 = 163.0;
        (inflation_rate / EPOCHS_PER_YEAR * staked_amount * vote_credit_proportion) as u64
    }

    /// Get Jito tip distribution program ID for the current cluster
    fn get_tip_distribution_program_id(&self) -> Pubkey {
        match self.cluster {
            Cluster::Testnet => Pubkey::from_str(TIP_DISTRIBUTION_PROGRAM_TESTNET).unwrap(),
            Cluster::MainnetBeta => Pubkey::from_str(TIP_DISTRIBUTION_PROGRAM_MAINNET).unwrap(),
        }
    }

    /// Get steward config public key
    fn get_steward_config_pubkey(&self) -> Pubkey {
        match &self.cluster {
            Cluster::Testnet => Pubkey::from_str(STEWARD_CONFIG_TESTNET).unwrap(),
            Cluster::MainnetBeta => Pubkey::from_str(STEWARD_CONFIG_MAINNET).unwrap(),
        }
    }

    /// Get validator history program ID for the current cluster
    fn get_validator_history_program_id(&self) -> Pubkey {
        match &self.cluster {
            Cluster::Testnet => Pubkey::from_str(VALIDATOR_HISTORY_PROGRAM_TESTNET).unwrap(),
            Cluster::MainnetBeta => Pubkey::from_str(VALIDATOR_HISTORY_PROGRAM_MAINNET).unwrap(),
        }
    }

    /// Get priority fee distribution program ID
    fn get_priority_fee_distribution_program_id(&self) -> solana_pubkey::Pubkey {
        solana_pubkey::Pubkey::from_str(PRIORITY_FEE_DISTRIBUTION_PROGRAM).unwrap()
    }
}
