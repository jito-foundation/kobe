use std::{str::FromStr, sync::Arc, thread::sleep, time::Duration as CoreDuration};

use chrono::{Duration, DurationRound, Utc};
use kobe_core::{
    constants::{MAINNET_STAKE_POOL_ADDRESS, TESTNET_STAKE_POOL_ADDRESS},
    db_models::{stake_pool_stats::StakePoolStats, validators::Validator},
    fetcher::{fetch_total_staked_lamports, ValidatorDataFetcher},
    validators_app::{Client, Cluster},
};
use log::info;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_program::{clock::DEFAULT_SLOTS_PER_EPOCH, pubkey::Pubkey};
use spl_stake_pool::state::StakePool;
use spl_stake_pool_cli::client::get_stake_pool;

use crate::{result::Result, rpc_utils};

pub struct StakePoolManager {
    /// RPC Client
    pub rpc_client: Arc<RpcClient>,

    /// Validator data fetcher
    validator_data_fetcher: ValidatorDataFetcher,

    pub validators_app_client: Client,
    pub cluster: Cluster,
}

impl StakePoolManager {
    pub fn new(
        rpc_client: Arc<RpcClient>,
        validator_data_fetcher: ValidatorDataFetcher,
        validators_app_client: Client,
        cluster: Cluster,
    ) -> Self {
        Self {
            rpc_client,
            validator_data_fetcher,
            validators_app_client,
            cluster,
        }
    }

    pub async fn fetch_all_validators(&self, epoch: u64) -> Result<Vec<Validator>> {
        let validators_app_client = self.validators_app_client.clone();
        let network_validators = tokio::task::spawn_blocking(move || {
            validators_app_client.validators(None, None, epoch)
        })
        .await??;

        let on_chain_data = self.validator_data_fetcher.fetch_chain_data(epoch).await?;

        let validators: Vec<Validator> = network_validators
            .as_ref()
            .iter()
            .filter(|v| v.epoch.is_some())
            .map(|v| {
                let on_chain_data_entry = on_chain_data
                    .get(&v.vote_account)
                    .expect("Each validator should have on chain data");
                Validator::new(v, on_chain_data_entry)
            })
            .collect();
        Ok(validators)
    }

    pub async fn get_mev_rewards(&self) -> Result<u64> {
        let rpc_client = &self.rpc_client;
        let current_epoch = rpc_utils::retry_get_epoch_info(rpc_client).await?;

        let total_mev_rewards = self
            .validator_data_fetcher
            .fetch_mev_rewards(current_epoch)
            .await?;

        Ok(total_mev_rewards)
    }

    pub async fn fetch_stake_pool_stats(
        &self,
        stake_pool_address: &Pubkey,
    ) -> Result<StakePoolStats> {
        let rpc_client = &self.rpc_client;
        let epoch = rpc_utils::retry_get_epoch_info(rpc_client).await?;
        let stake_pool = get_stake_pool(rpc_client, stake_pool_address).await?;
        let vote_accounts = rpc_client.get_vote_accounts().await?;

        let recent_slot_ms = rpc_utils::get_slot_times(rpc_client, epoch)
            .await
            .unwrap_or(400);
        let stats = StakePoolStats {
            epoch,
            num_deposits: 0,
            reserve_balance: rpc_utils::get_reserve_balance(rpc_client, &stake_pool).await?,
            timestamp: Utc::now(),
            total_solana_lamports: stake_pool.total_lamports,
            total_pool_lamports: stake_pool.pool_token_supply,
            apy: get_stake_pool_apy(&stake_pool, recent_slot_ms),
            num_validators: rpc_utils::get_num_validators(rpc_client, &stake_pool.validator_list)
                .await?,
            mev_rewards: self.get_mev_rewards().await?,
            fees_collected: rpc_utils::get_stake_pool_fees(rpc_client, &stake_pool)
                .await
                .ok(),
            total_network_staked_lamports: Some(fetch_total_staked_lamports(&vote_accounts)),
        };

        info!("Done writing stats: {stats:#?}");

        Ok(stats)
    }
}

/// Simple APY calculation based on previous epoch and current epoch values
pub fn get_stake_pool_apy(stake_pool: &StakePool, slot_ms: u64) -> f64 {
    let seconds_per_epoch = DEFAULT_SLOTS_PER_EPOCH * slot_ms / 1000;
    let epochs_per_year = 365.25 * 3600.0 * 24.0 / seconds_per_epoch as f64;
    let epoch_rate = (stake_pool.total_lamports as f64 / stake_pool.pool_token_supply as f64)
        / (stake_pool.last_epoch_total_lamports as f64
            / stake_pool.last_epoch_pool_token_supply as f64);

    epoch_rate.powf(epochs_per_year) - 1.0
}

pub fn wait_for_next_duration(duration: Duration) {
    let mut now = Utc::now();

    let next_time = now + duration;
    let next_time_trunc = next_time.duration_trunc(duration).unwrap();
    info!("Waiting until {next_time_trunc:#} to begin next run");

    loop {
        sleep(CoreDuration::from_secs(60));
        if now > next_time_trunc {
            return;
        }
        now = Utc::now();
    }
}

pub fn resolve_stake_pool_address(cluster: &Cluster) -> Result<Pubkey> {
    let address_str = match cluster {
        Cluster::Testnet => TESTNET_STAKE_POOL_ADDRESS,
        Cluster::MainnetBeta => MAINNET_STAKE_POOL_ADDRESS,
    };
    Ok(Pubkey::from_str(address_str)?)
}
