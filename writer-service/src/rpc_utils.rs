use std::{env, time::Duration as CoreDuration};

use backoff::ExponentialBackoff;
use kobe_core::validators_app::Cluster;
use log::error;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program::{clock::DEFAULT_SLOTS_PER_EPOCH, pubkey::Pubkey};
use solana_sdk::{borsh1::try_from_slice_unchecked, commitment_config::CommitmentConfig};
use spl_stake_pool::state::{StakePool, ValidatorList};

use crate::result::{AppError, Result};

pub async fn get_num_validators(
    rpc_client: &RpcClient,
    validator_list_address: &Pubkey,
) -> Result<u32> {
    let account_data = rpc_client.get_account_data(validator_list_address).await?;
    let validator_list = try_from_slice_unchecked::<ValidatorList>(account_data.as_slice())
        .map_err(|e| {
            error!("Failed to get validator list: {e:?}");
            AppError::NotFound(e.to_string())
        })?;

    Ok(validator_list.validators.len() as u32)
}

// returns the reserve balance of the reserve account (in lamports)
pub async fn get_reserve_balance(rpc_client: &RpcClient, stake_pool: &StakePool) -> Result<u64> {
    let reserve_account = stake_pool.reserve_stake;
    let balance = rpc_client.get_balance(&reserve_account).await?;
    Ok(balance)
}

pub async fn get_stake_pool_fees(rpc_client: &RpcClient, stake_pool: &StakePool) -> Result<f64> {
    let fee_account = stake_pool.manager_fee_account;
    let fee_account_balance = rpc_client.get_token_account_balance(&fee_account).await?;

    fee_account_balance
        .ui_amount
        .ok_or(AppError::EmptyFeeAccountBalance(fee_account.to_string()))
}

pub async fn find_next_block(rpc_client: &RpcClient, slot: u64) -> Result<u64> {
    let slots = rpc_client.get_blocks(slot, Some(slot + 1000)).await?;
    slots.into_iter().min().ok_or(AppError::SlotNotFound)
}

pub async fn get_slot_times(rpc_client: &RpcClient, epoch: u64) -> Result<u64> {
    // Gets average slot times for the previous epoch based on start and end block times.
    // If no slot was found for the start and end block, seek forward until a block is found.
    // Rounds to nearest millisecond
    let prev_epoch_start_slot = DEFAULT_SLOTS_PER_EPOCH * (epoch - 1);
    let current_epoch_start_slot = DEFAULT_SLOTS_PER_EPOCH * (epoch);

    let prev_epoch_first_block = find_next_block(rpc_client, prev_epoch_start_slot).await?;
    let current_epoch_first_block = find_next_block(rpc_client, current_epoch_start_slot).await?;

    // Unix epoch time in seconds
    let start_time = rpc_client.get_block_time(prev_epoch_first_block).await?;
    let end_time = rpc_client.get_block_time(current_epoch_first_block).await?;
    let slot_ms = 1000. * (end_time - start_time) as f64
        / (current_epoch_first_block - prev_epoch_first_block) as f64;
    Ok(slot_ms as u64)
}

pub async fn retry_get_epoch_info(rpc_client: &RpcClient) -> Result<u64> {
    let backoff = ExponentialBackoff::default();
    let op = || async {
        match rpc_client
            .get_epoch_info_with_commitment(CommitmentConfig::default())
            .await
        {
            Ok(epoch_info) => Ok(epoch_info.epoch),
            Err(e) => Err(backoff::Error::Transient {
                err: e,
                retry_after: None,
            }),
        }
    };
    backoff::future::retry(backoff, op)
        .await
        .map_err(|e| e.into())
}

fn get_rpc_url(cluster: &Cluster) -> String {
    let mainnet_url = env::var("MAINNET_CLUSTER_URL")
        .unwrap_or("https://api.mainnet-beta.solana.com".to_string());

    let testnet_url =
        env::var("TESTNET_CLUSTER_URL").unwrap_or("https://api.testnet.solana.com".to_string());

    let devnet_url =
        env::var("DEVNET_CLUSTER_URL").unwrap_or("https://api.devnet.solana.com".to_string());

    let json_rpc_url = match cluster {
        Cluster::Devnet => devnet_url,
        Cluster::MainnetBeta => mainnet_url,
        Cluster::Testnet => testnet_url,
    };

    json_rpc_url.to_string()
}

/// Set up and configures and RPC client
pub fn setup_rpc_client(cluster: &Cluster, rpc_url: Option<String>) -> Result<RpcClient> {
    let json_rpc_url = match rpc_url {
        Some(url) => url,
        None => get_rpc_url(cluster),
    };

    Ok(RpcClient::new_with_timeout_and_commitment(
        json_rpc_url,
        CoreDuration::from_secs(60),
        CommitmentConfig::confirmed(),
    ))
}
