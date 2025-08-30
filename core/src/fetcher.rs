use std::{collections::HashMap, str::FromStr, sync::Arc};

use anchor_lang::AccountDeserialize;
use bincode::deserialize;
use jito_priority_fee_distribution::state::PriorityFeeDistributionAccount;
use jito_priority_fee_distribution_sdk::derive_priority_fee_distribution_account_address;
use jito_tip_distribution::state::TipDistributionAccount;
use log::*;
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
use stakenet_sdk::utils::accounts::get_all_validator_history_accounts;
use validator_history::ValidatorHistory;

use crate::{
    client_type::ClientType,
    constants::{
        PRIORITY_FEE_DISTRIBUTION_PROGRAM, TIP_DISTRIBUTION_PROGRAM_MAINNET,
        TIP_DISTRIBUTION_PROGRAM_TESTNET, VALIDATOR_HISTORY_PROGRAM_MAINNET,
        VALIDATOR_HISTORY_PROGRAM_TESTNET,
    },
    validators_app::Cluster,
};

type Error = Box<dyn std::error::Error>;

pub struct ValidatorDataFetcher {
    /// RPC Client
    rpc_client: Arc<RpcClient>,

    /// Cluster
    cluster: Cluster,

    /// Validator list pubkey
    validator_list_pubkey: Pubkey,
}

#[derive(Default, Debug)]
pub struct ValidatorChainData {
    /// Validator's name
    pub name: Option<String>,

    /// Website
    pub website: Option<String>,

    /// MEV commission BPS
    pub mev_commission_bps: Option<u16>,
    pub mev_revenue_lamports: u64,
    pub running_jito: bool,
    pub vote_credit_proportion: f64,
    pub stake_info: Option<ValidatorStakeInfo>,
    pub total_staked_lamports: u64,
    pub inflation_rewards_lamports: u64,
    pub priority_fee_commission_bps: u16,
    pub priority_fee_revenue_lamports: u64,
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

    /// Fetch all validators info
    ///
    /// Returns the HashMap of node pubkey, validator info
    /// Look like:
    ///[2025-08-29T20:22:59Z INFO  kobe_core::fetcher] UiConfig { keys: [UiConfigKey { pubkey: "Va1idator1nfo111111111111111111111111111111", signer: false }, UiConfigKey { pubkey: "3WyA7gWkTwdeYLqyQsX1tS9LoC51VdA2SX5LbqwwWRzm", signer: true }], config_data: Object {"details": String("Power up your staking rewards"), "iconUrl": String("https://i.imgur.com/6CI235o.jpeg"), "name": String("Chainsaw"), "website": String("https://chainsaw-solana.com/")} }
    async fn fetch_validator_info(
        &self,
        vote_accounts: &RpcVoteAccountStatus,
        validators_chain_data: &mut HashMap<Pubkey, ValidatorChainData>,
    ) -> Result<(), Error> {
        let all_config = self
            .rpc_client
            .get_program_accounts(&solana_config_interface::id())
            .await
            .unwrap();
        let validator_info: Vec<(Pubkey, Account)> = all_config
            .into_iter()
            .filter(|(_, validator_info_account)| {
                match deserialize::<ConfigKeys>(&validator_info_account.data) {
                    Ok(key_list) => key_list.keys.contains(&(validator_info::id(), false)),
                    Err(_) => false,
                }
            })
            .collect();

        for (pubkey, info) in validator_info {
            if let Ok(ConfigAccountType::ValidatorInfo(value)) = parse_config(&info.data, &pubkey) {
                if let Some(key) = value.keys.get(1) {
                    for vote_account in vote_accounts.current.iter() {
                        if vote_account.node_pubkey.eq(&key.pubkey) {
                            let chain_data = ValidatorChainData {
                                name: value.config_data.get("name").map(|name| name.to_string()),
                                website: value
                                    .config_data
                                    .get("website")
                                    .map(|website| website.to_string()),
                                ..Default::default()
                            };
                            validators_chain_data
                                .insert(Pubkey::from_str(&vote_account.vote_pubkey)?, chain_data);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Fetch all tip distribution accounts
    ///
    /// Set the commission rate on each validator if it exists. That means this validator is running jito this epoch
    /// Commission rate also used for scoring
    /// Done in batches for efficiency
    ///
    /// # Retruns
    ///
    /// HashMap of key: vote account, value: tip distributioin account
    async fn fetch_tip_distribution_accounts(
        &self,
        validator_vote_pubkeys: &[Pubkey],
        epoch: u64,
    ) -> Result<HashMap<Pubkey, TipDistributionAccount>, Error> {
        let mut commission_map = HashMap::new();
        for chunk in validator_vote_pubkeys.chunks(100) {
            let pubkeys = chunk
                .iter()
                .map(|c| {
                    jito_tip_distribution_sdk::derive_tip_distribution_account_address(
                        &self.get_tip_distribution_program_id(),
                        c,
                        epoch,
                    )
                    .0
                })
                .collect::<Vec<Pubkey>>();
            let response = self.rpc_client.get_multiple_accounts(&pubkeys).await;
            if let Ok(result) = response {
                for (v, acc) in core::iter::zip(chunk, result) {
                    if let Some(account) = acc {
                        if self.get_tip_distribution_program_id() != account.owner {
                            warn!("Validator {} may be trying to mess with their Tip Distribution Account", v);
                            continue;
                        }
                        let tip_distribution =
                            TipDistributionAccount::try_deserialize(&mut account.data.as_slice())?;

                        commission_map.insert(*v, tip_distribution);
                    }
                }
            } else if let Err(e) = response {
                error!("Rpc error: {e:#?}");
            }
        }

        Ok(commission_map)
    }

    async fn fetch_priority_fee_distribution_accounts(
        &self,
        validator_vote_pubkeys: &[Pubkey],
        epoch: u64,
    ) -> Result<HashMap<Pubkey, PriorityFeeDistributionAccount>, Error> {
        // Set the commission rate on each validator if it exists. That means this validator is running jito this epoch
        // Commission rate also used for scoring
        // Done in batches for efficiency
        let mut commission_map = HashMap::new();
        for chunk in validator_vote_pubkeys.chunks(100) {
            let pubkeys = chunk
                .iter()
                .map(|c| {
                    let vote_account_pubkey =
                        solana_pubkey::Pubkey::from_str(&c.to_string()).unwrap();
                    let pfda_pubkey: solana_pubkey::Pubkey =
                        derive_priority_fee_distribution_account_address(
                            &self.get_priority_fee_distribution_program_id(),
                            &vote_account_pubkey,
                            epoch,
                        )
                        .0;
                    Pubkey::from_str(&pfda_pubkey.to_string()).unwrap()
                })
                .collect::<Vec<Pubkey>>();
            let response = self.rpc_client.get_multiple_accounts(&pubkeys).await;
            if let Ok(result) = response {
                for (v, acc) in core::iter::zip(chunk, result) {
                    if let Some(account) = acc {
                        let program_pubkey = Pubkey::from_str(
                            &self.get_priority_fee_distribution_program_id().to_string(),
                        )
                        .unwrap();
                        if program_pubkey != account.owner {
                            warn!("Validator {} may be trying to mess with their Tip Distribution Account", v);
                            continue;
                        }
                        let tip_distribution = PriorityFeeDistributionAccount::try_deserialize(
                            &mut account.data.as_slice(),
                        )?;
                        commission_map.insert(*v, tip_distribution);
                    }
                }
            } else if let Err(e) = response {
                error!("Rpc error: {e:#?}");
            }
        }

        Ok(commission_map)
    }

    /// Fetch all [`ValidatorHistory`] accounts and retruns thems as a lookup map
    ///
    /// ## Overview
    ///
    /// This function retrieves all validator history accounts for the specified program and creates a
    /// HashMap indexed by vote account pubkey
    async fn fetch_validator_history_accounts(
        &self,
    ) -> Result<HashMap<Pubkey, ValidatorHistory>, Error> {
        let validator_history_program_id = self.get_validator_history_program_id();
        let mut validator_history_map: HashMap<Pubkey, ValidatorHistory> = HashMap::new();
        let validator_histories =
            get_all_validator_history_accounts(&self.rpc_client, validator_history_program_id)
                .await?;

        for validator_history in validator_histories {
            validator_history_map.insert(validator_history.vote_account, validator_history);
        }

        Ok(validator_history_map)
    }

    pub async fn fetch_mev_rewards(&self, epoch: u64) -> Result<u64, Error> {
        let vote_accounts = self.rpc_client.get_vote_accounts().await?;
        let validator_vote_pubkeys: Vec<Pubkey> = vote_accounts
            .current
            .iter()
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
                .map(|c| {
                    jito_tip_distribution_sdk::derive_tip_distribution_account_address(
                        &self.get_tip_distribution_program_id(),
                        c,
                        epoch,
                    )
                    .0
                })
                .collect::<Vec<Pubkey>>();
            let response = self.rpc_client.get_multiple_accounts(&pubkeys).await;
            if let Ok(result) = response {
                for (v, acc) in core::iter::zip(chunk, result) {
                    if let Some(account) = acc {
                        if self.get_tip_distribution_program_id() != account.owner {
                            warn!("Validator {} may be trying to mess with their Tip Distribution Account", v);
                            continue;
                        }

                        total += account.lamports - tip_distribution_account_rent;
                    }
                }
            } else if let Err(e) = response {
                error!("Rpc error: {e:#?}");
            }
        }
        Ok(total)
    }

    /// Fetches on-chain data for a set of validators
    ///
    /// # Overview
    ///
    /// Aggregate multiple types of on-chain data including MEV distributions, vote credits, staking
    /// information, and validator history to build a complete picture of validator performance and
    /// economics for a given epoch.
    ///
    /// # Data Sources
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
    pub async fn fetch_chain_data(
        &self,
        epoch: u64,
    ) -> Result<HashMap<Pubkey, ValidatorChainData>, Error> {
        // Fetch on-chain data
        let vote_accounts = self.rpc_client.get_vote_accounts().await?;

        let mut validators_chain_data = HashMap::new();

        self.fetch_validator_info(&vote_accounts, &mut validators_chain_data)
            .await?;

        let validator_vote_pubkeys: Vec<Pubkey> = validators_chain_data.keys().copied().collect();

        let tip_distributions = self
            .fetch_tip_distribution_accounts(&validator_vote_pubkeys, epoch)
            .await?;

        let priority_fee_distributions = self
            .fetch_priority_fee_distribution_accounts(&validator_vote_pubkeys, epoch)
            .await?;

        let (global_average, vote_credits_map) = fetch_vote_credits(&vote_accounts)?;

        let total_staked_lamports = fetch_total_staked_lamports(&vote_accounts);

        let staked_validators =
            get_validator_list(&self.rpc_client, &self.validator_list_pubkey).await?;
        let inflation_rate = match self.rpc_client.get_inflation_rate().await {
            Ok(rate) => rate.total,
            Err(e) => {
                error!("Failed to fetch inflation rate: {e:#?}");
                0.
            }
        };

        let validator_histories = self.fetch_validator_history_accounts().await?;

        for (vote_account, chain_data) in validators_chain_data.iter_mut() {
            // let vote_account = v.vote_account;
            let maybe_tip_distribution_account = tip_distributions.get(vote_account);

            let has_tip_account = maybe_tip_distribution_account.is_some();
            let is_jito_client = validator_histories
                .get(vote_account)
                .map(|validator_history| {
                    validator_history
                        .history
                        .arr
                        .iter()
                        .find(|entry| entry.epoch.eq(&(epoch as u16)))
                        .map(|entry| {
                            matches!(ClientType::from_u8(entry.client_type), ClientType::JitoLabs)
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            let running_jito = has_tip_account || is_jito_client;

            let (mev_commission_bps, mev_revenue_lamports) =
                if let Some(tda) = maybe_tip_distribution_account {
                    let mev_revenue = if let Some(merkle_root) = tda.merkle_root.clone() {
                        merkle_root.max_total_claim
                    } else {
                        0
                    };
                    (Some(tda.validator_commission_bps), mev_revenue)
                } else {
                    (None, 0)
                };

            let maybe_priority_fee_distribution_account =
                priority_fee_distributions.get(vote_account);
            let (priority_fee_commission_bps, priority_fee_revenue_lamports) =
                if let Some(pfda) = maybe_priority_fee_distribution_account {
                    let fee_revenue = if let Some(merkle_root) = pfda.merkle_root.clone() {
                        merkle_root.max_total_claim
                    } else {
                        0
                    };
                    (pfda.validator_commission_bps, fee_revenue)
                } else {
                    (10000, 0)
                };
            let vote_credits = *vote_credits_map.get(vote_account).unwrap_or(&0.);
            let vote_credit_proportion = vote_credits / global_average;
            let stake_info = staked_validators
                .validators
                .clone()
                .into_iter()
                .find(|info| *vote_account == info.vote_account_address);

            // hardcoded from cogentcrypto.io
            let epochs_per_year = 163.;

            let staked_amount =
                fetch_staked_lamports_for_validator(&vote_accounts, vote_account) as f64;

            let inflation_rewards_lamports =
                inflation_rate / epochs_per_year * staked_amount * vote_credit_proportion;

            chain_data.mev_commission_bps = mev_commission_bps;
            chain_data.mev_revenue_lamports = mev_revenue_lamports;
            chain_data.running_jito = running_jito;
            chain_data.vote_credit_proportion = vote_credit_proportion;
            chain_data.stake_info = stake_info;
            chain_data.total_staked_lamports = total_staked_lamports;
            chain_data.inflation_rewards_lamports = inflation_rewards_lamports as u64;
            chain_data.priority_fee_commission_bps = priority_fee_commission_bps;
            chain_data.priority_fee_revenue_lamports = priority_fee_revenue_lamports;
        }

        Ok(validators_chain_data)
    }

    /// Get Tip Distribution Program ID
    pub fn get_tip_distribution_program_id(&self) -> Pubkey {
        // These seem to be in flux
        match self.cluster {
            Cluster::Testnet => Pubkey::from_str(TIP_DISTRIBUTION_PROGRAM_TESTNET).unwrap(),
            Cluster::MainnetBeta => Pubkey::from_str(TIP_DISTRIBUTION_PROGRAM_MAINNET).unwrap(),
        }
    }

    /// Get validator history program ID
    pub fn get_validator_history_program_id(&self) -> Pubkey {
        match &self.cluster {
            Cluster::Testnet => Pubkey::from_str(VALIDATOR_HISTORY_PROGRAM_TESTNET).unwrap(),
            Cluster::MainnetBeta => Pubkey::from_str(VALIDATOR_HISTORY_PROGRAM_MAINNET).unwrap(),
        }
    }

    pub fn get_priority_fee_distribution_program_id(&self) -> solana_pubkey::Pubkey {
        solana_pubkey::Pubkey::from_str(PRIORITY_FEE_DISTRIBUTION_PROGRAM).unwrap()
    }
}

// fetches global average vote credits and average vote credits per validator over last 5 epochs
pub fn fetch_vote_credits(
    vote_accounts: &RpcVoteAccountStatus,
) -> Result<(f64, HashMap<Pubkey, f64>), Error> {
    // Loop through vote accounts
    let mut average_vote_credits_map: HashMap<Pubkey, f64> = HashMap::new();
    for vote_account in vote_accounts
        .current
        .iter()
        .chain(vote_accounts.delinquent.iter())
    {
        // Calculate average
        let pubkey = Pubkey::from_str(&vote_account.vote_pubkey)?;
        let sum = vote_account
            .epoch_credits
            .iter()
            .map(|(_, current, prev)| current - prev)
            .sum::<u64>();
        let average = if vote_account.epoch_credits.is_empty() {
            0.
        } else {
            sum as f64 / vote_account.epoch_credits.len() as f64
        };
        average_vote_credits_map.insert(pubkey, average);
    }
    let global_average =
        average_vote_credits_map.values().sum::<f64>() / average_vote_credits_map.len() as f64;
    Ok((global_average, average_vote_credits_map))
}

pub fn fetch_total_staked_lamports(vote_accounts: &RpcVoteAccountStatus) -> u64 {
    vote_accounts
        .current
        .iter()
        .chain(vote_accounts.delinquent.iter())
        .map(|info| info.activated_stake)
        .sum()
}

pub fn fetch_staked_lamports_for_validator(
    vote_accounts: &RpcVoteAccountStatus,
    vote_account: &Pubkey,
) -> u64 {
    vote_accounts
        .current
        .iter()
        .chain(vote_accounts.delinquent.iter())
        .find(|info| &Pubkey::from_str(&info.vote_pubkey).unwrap() == vote_account)
        .map(|info| info.activated_stake)
        .unwrap_or(0)
}
