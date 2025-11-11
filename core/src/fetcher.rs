use std::{
    collections::{HashMap, HashSet},
    str::FromStr, sync::Arc
};

use anchor_lang::AccountDeserialize;
use jito_priority_fee_distribution::state::PriorityFeeDistributionAccount;
use jito_priority_fee_distribution_sdk::derive_priority_fee_distribution_account_address;
use jito_tip_distribution::state::TipDistributionAccount;
use log::*;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_response::RpcVoteAccountStatus};
use solana_pubkey::Pubkey;
use spl_stake_pool::state::ValidatorStakeInfo;
use spl_stake_pool_cli::client::get_validator_list;
use stakenet_sdk::utils::accounts::{
    get_all_validator_history_accounts, get_directed_stake_meta, get_steward_config_account,
};
use validator_history::{ValidatorHistory, ValidatorHistoryEntry};

use crate::{
    client_type::ClientType,
    constants::{
        PRIORITY_FEE_DISTRIBUTION_PROGRAM, TIP_DISTRIBUTION_PROGRAM_MAINNET,
        TIP_DISTRIBUTION_PROGRAM_TESTNET, VALIDATOR_HISTORY_PROGRAM_MAINNET,
        VALIDATOR_HISTORY_PROGRAM_TESTNET,
    },
    validators_app::{Cluster, ValidatorsAppResponseEntry},
};

type Error = Box<dyn std::error::Error>;

#[derive(Default, Debug)]
pub struct ChainData {
    /// MEV commission BPS
    pub mev_commission_bps: Option<u16>,
    pub mev_revenue_lamports: u64,

    /// Whether or not running Jito client
    pub running_jito: bool,

    /// Whether or not running BAM client
    pub running_bam: bool,

    pub vote_credit_proportion: f64,
    pub stake_info: Option<ValidatorStakeInfo>,
    pub total_staked_lamports: u64,
    pub inflation_rewards_lamports: u64,
    pub priority_fee_commission_bps: u16,
    pub priority_fee_revenue_lamports: u64,

    /// Jito Pool eligible
    pub jito_pool_eligible: bool,

    /// Jito Directed Stake Target
    pub jito_directed_stake_target: bool,
}

pub fn get_tip_distribution_program_id(cluster: &Cluster) -> Pubkey {
    // These seem to be in flux
    match cluster {
        Cluster::Localhost => unimplemented!(),
        Cluster::Devnet => unimplemented!(),
        Cluster::Testnet => Pubkey::from_str(TIP_DISTRIBUTION_PROGRAM_TESTNET).unwrap(),
        Cluster::MainnetBeta => Pubkey::from_str(TIP_DISTRIBUTION_PROGRAM_MAINNET).unwrap(),
    }
}

/// Get validator history program ID
pub fn get_validator_history_program_id(cluster: &Cluster) -> Pubkey {
    match cluster {
        Cluster::Localhost => unimplemented!(),
        Cluster::Devnet => unimplemented!(),
        Cluster::Testnet => Pubkey::from_str(VALIDATOR_HISTORY_PROGRAM_TESTNET).unwrap(),
        Cluster::MainnetBeta => Pubkey::from_str(VALIDATOR_HISTORY_PROGRAM_MAINNET).unwrap(),
    }
}

pub fn get_priority_fee_distribution_program_id() -> solana_pubkey::Pubkey {
    solana_pubkey::Pubkey::from_str(PRIORITY_FEE_DISTRIBUTION_PROGRAM).unwrap()
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
///
/// ## BAM Client Detection
///
/// - If BAM validator set exists and not empty: check if validator identity is in the set
/// - Otherwise: fall back to client_type check from validator history
pub async fn fetch_chain_data(
    validators: &[ValidatorsAppResponseEntry],
    bam_validator_set: HashSet<String>,
    rpc_client: Arc<RpcClient>,
    cluster: &Cluster,
    epoch: u64,
    validator_list_pubkey: &Pubkey,
    jito_steward_program_id: &Pubkey,
    steward_config_pubkey: &Pubkey,
) -> Result<HashMap<Pubkey, ChainData>, Error> {
    // Fetch on-chain data
    let tip_distributions =
        fetch_tip_distribution_accounts(validators, &rpc_client.clone(), cluster, epoch).await?;
    let priority_fee_distributions =
        fetch_priority_fee_distribution_accounts(validators, &rpc_client.clone(), epoch).await?;
    let vote_accounts = rpc_client.get_vote_accounts().await?;
    let (global_average, vote_credits_map) = fetch_vote_credits(&vote_accounts)?;

    let total_staked_lamports = fetch_total_staked_lamports(&vote_accounts);

    let steward_config =
        get_steward_config_account(&rpc_client.clone(), steward_config_pubkey).await?;

    let staked_validators = get_validator_list(&rpc_client.clone(), validator_list_pubkey).await?;
    let inflation_rate = match rpc_client.get_inflation_rate().await {
        Ok(rate) => rate.total,
        Err(e) => {
            error!("Failed to fetch inflation rate: {e:#?}");
            0.
        }
    };

    let validator_history_program_id = get_validator_history_program_id(cluster);
    let validator_histories =
        fetch_validator_history_accounts(&rpc_client.clone(), validator_history_program_id).await?;

    let directed_stake_meta = get_directed_stake_meta(
        rpc_client.clone(),
        steward_config_pubkey,
        jito_steward_program_id,
    )
    .await?;

    Ok(HashMap::from_iter(validators.iter().map(|v| {
        let vote_account = v.vote_account;
        let maybe_tip_distribution_account = tip_distributions.get(&vote_account);

        let has_tip_account = maybe_tip_distribution_account.is_some();
        let client_type = validator_histories
            .get(&vote_account)
            .and_then(|validator_history| {
                validator_history
                    .history
                    .arr
                    .iter()
                    .find(|entry| entry.epoch == epoch as u16)
            })
            .map(|entry| ClientType::from_u8(entry.client_type));
        let is_jito_client = matches!(client_type, Some(ClientType::JitoLabs));

        let is_bam_client = match (&v.account, bam_validator_set.is_empty()) {
            (Some(identity), false) => bam_validator_set.contains(identity.as_str()),
            _ => matches!(client_type, Some(ClientType::Bam)),
        };
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

        let maybe_priority_fee_distribution_account = priority_fee_distributions.get(&vote_account);
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
        let vote_credits = *vote_credits_map.get(&vote_account).unwrap_or(&0.);
        let vote_credit_proportion = vote_credits / global_average;
        let stake_info = staked_validators
            .validators
            .clone()
            .into_iter()
            .find(|info| v.vote_account == info.vote_account_address);

        // hardcoded from cogentcrypto.io
        let epochs_per_year = 163.;

        let staked_amount =
            fetch_staked_lamports_for_validator(&vote_accounts, &vote_account) as f64;

        let inflation_rewards_lamports =
            inflation_rate / epochs_per_year * staked_amount * vote_credit_proportion;

        let start_epoch = epoch.saturating_sub(
            steward_config
                .parameters
                .minimum_voting_epochs
                .saturating_sub(1),
        );

        let mut jito_pool_eligible = true;
        if let Some(validator_history) = validator_histories.get(&vote_account) {
            if validator_history
                .history
                .epoch_credits_range(start_epoch as u16, epoch as u16)
                .iter()
                .any(|entry| entry.is_none())
            {
                jito_pool_eligible = false;
            }

            if let Some(entry) = validator_history.history.last() {
                if entry
                    .activated_stake_lamports
                    .eq(&ValidatorHistoryEntry::default().activated_stake_lamports)
                {
                    jito_pool_eligible = false;
                }

                if entry.activated_stake_lamports < steward_config.parameters.minimum_stake_lamports
                {
                    jito_pool_eligible = false;
                }
            }
        }

        let jito_directed_stake_target = directed_stake_meta
            .targets
            .iter()
            .any(|target| target.vote_pubkey.eq(&v.vote_account));

        let data = ChainData {
            mev_commission_bps,
            mev_revenue_lamports,
            running_jito,
            running_bam: is_bam_client,
            vote_credit_proportion,
            stake_info,
            total_staked_lamports,
            inflation_rewards_lamports: inflation_rewards_lamports as u64,
            priority_fee_commission_bps,
            priority_fee_revenue_lamports,
            jito_pool_eligible,
            jito_directed_stake_target,
        };

        (vote_account, data)
    })))
}

pub async fn fetch_tip_distribution_accounts(
    validators: &[ValidatorsAppResponseEntry],
    rpc_client: &RpcClient,
    cluster: &Cluster,
    epoch: u64,
) -> Result<HashMap<Pubkey, TipDistributionAccount>, Error> {
    // Set the commission rate on each validator if it exists. That means this validator is running jito this epoch
    // Commission rate also used for scoring
    // Done in batches for efficiency
    let mut commission_map = HashMap::new();
    for chunk in validators.chunks(100) {
        let pubkeys = chunk
            .iter()
            .map(|c| {
                jito_tip_distribution_sdk::derive_tip_distribution_account_address(
                    &get_tip_distribution_program_id(cluster),
                    &c.vote_account,
                    epoch,
                )
                .0
            })
            .collect::<Vec<Pubkey>>();
        let response = rpc_client.get_multiple_accounts(&pubkeys).await;
        if let Ok(result) = response {
            for (v, acc) in core::iter::zip(chunk, result) {
                if let Some(account) = acc {
                    if get_tip_distribution_program_id(cluster) != account.owner {
                        warn!("Validator {} may be trying to mess with their Tip Distribution Account", v.vote_account);
                        continue;
                    }
                    let tip_distribution =
                        TipDistributionAccount::try_deserialize(&mut account.data.as_slice())?;
                    commission_map.insert(v.vote_account, tip_distribution);
                }
            }
        } else if let Err(e) = response {
            error!("Rpc error: {e:#?}");
        }
    }
    Ok(commission_map)
}

pub async fn fetch_priority_fee_distribution_accounts(
    validators: &[ValidatorsAppResponseEntry],
    rpc_client: &RpcClient,
    epoch: u64,
) -> Result<HashMap<Pubkey, PriorityFeeDistributionAccount>, Error> {
    // Set the commission rate on each validator if it exists. That means this validator is running jito this epoch
    // Commission rate also used for scoring
    // Done in batches for efficiency
    let mut commission_map = HashMap::new();
    for chunk in validators.chunks(100) {
        let pubkeys = chunk
            .iter()
            .map(|c| {
                let vote_account_pubkey =
                    solana_pubkey::Pubkey::from_str(&c.vote_account.to_string()).unwrap();
                let pfda_pubkey: solana_pubkey::Pubkey =
                    derive_priority_fee_distribution_account_address(
                        &get_priority_fee_distribution_program_id(),
                        &vote_account_pubkey,
                        epoch,
                    )
                    .0;
                Pubkey::from_str(&pfda_pubkey.to_string()).unwrap()
            })
            .collect::<Vec<Pubkey>>();
        let response = rpc_client.get_multiple_accounts(&pubkeys).await;
        if let Ok(result) = response {
            for (v, acc) in core::iter::zip(chunk, result) {
                if let Some(account) = acc {
                    let program_pubkey =
                        Pubkey::from_str(&get_priority_fee_distribution_program_id().to_string())
                            .unwrap();
                    if program_pubkey != account.owner {
                        warn!("Validator {} may be trying to mess with their Tip Distribution Account", v.vote_account);
                        continue;
                    }
                    let tip_distribution = PriorityFeeDistributionAccount::try_deserialize(
                        &mut account.data.as_slice(),
                    )?;
                    commission_map.insert(v.vote_account, tip_distribution);
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
pub async fn fetch_validator_history_accounts(
    rpc_client: &RpcClient,
    program_id: Pubkey,
) -> Result<HashMap<Pubkey, ValidatorHistory>, Error> {
    let mut validator_history_map: HashMap<Pubkey, ValidatorHistory> = HashMap::new();
    let validator_histories = get_all_validator_history_accounts(rpc_client, program_id).await?;

    for validator_history in validator_histories {
        validator_history_map.insert(validator_history.vote_account, validator_history);
    }

    Ok(validator_history_map)
}

pub async fn fetch_mev_rewards(
    validators: &[ValidatorsAppResponseEntry],
    rpc_client: &RpcClient,
    cluster: &Cluster,
    epoch: u64,
) -> Result<u64, Error> {
    let mut total = 0;
    let tip_distribution_account_rent = rpc_client
        .get_minimum_balance_for_rent_exemption(TipDistributionAccount::SIZE)
        .await?;
    for chunk in validators.chunks(100) {
        let pubkeys = chunk
            .iter()
            .map(|c| {
                jito_tip_distribution_sdk::derive_tip_distribution_account_address(
                    &get_tip_distribution_program_id(cluster),
                    &c.vote_account,
                    epoch,
                )
                .0
            })
            .collect::<Vec<Pubkey>>();
        let response = rpc_client.get_multiple_accounts(&pubkeys).await;
        if let Ok(result) = response {
            for (v, acc) in core::iter::zip(chunk, result) {
                if let Some(account) = acc {
                    if get_tip_distribution_program_id(cluster) != account.owner {
                        warn!("Validator {} may be trying to mess with their Tip Distribution Account", v.vote_account);
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
