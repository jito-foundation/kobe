use std::{sync::Arc, time::Duration};

use kobe_core::{
    constants::DATABASE_NAME,
    validators_app::{Client as ValidatorsAppClient, Cluster},
};
use log::{error, info};
use mongodb::Database;
use rand::Rng;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_metrics::datapoint_info;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use spl_stake_pool::state::StakePool;
use spl_stake_pool_cli::client::get_stake_pool;
use tokio::time::{sleep_until, Instant};

use crate::{
    bam_boost_manager::BamBoostManager,
    db::{
        write_bam_snapshot_info, write_mev_claims_info, write_stake_pool_info, write_validator_info,
    },
    result::Result,
    stake_pool_manager::StakePoolManager,
};

pub mod bam_boost_manager;
pub mod db;
pub mod google_storage;
pub mod merkle_tree_parser;
pub mod result;
pub mod rpc_utils;
pub mod stake_pool_manager;
pub mod tip_distributor_sdk;

/// Target spacing between BAM connection snapshots.
const BAM_SNAPSHOT_BASE_SECS: u64 = 1800; // 30 minutes

/// Maximum random offset applied to each BAM snapshot interval, in either direction. Spreading
/// the snapshot away from an exact 30-minute cadence makes it harder to predict when a snapshot
/// is taken (and therefore harder to game the connection-rate metric).
const BAM_SNAPSHOT_JITTER_SECS: u64 = 300; // +/- 5 minutes

/// Kobe writer service main instance
pub struct KobeWriterService {
    /// Monogo DB instance
    db: Database,

    /// Stake pool manager
    stake_pool_manager: StakePoolManager,

    /// BAM Boost manager
    bam_boost_manager: BamBoostManager,

    /// Stake pool
    stake_pool: StakePool,

    /// Stake pool address
    pub stake_pool_address: Pubkey,

    /// Cluster name
    cluster: Cluster,

    /// Tip distribution program id
    tip_distribution_program_id: String,

    /// Priority fee distribution program id
    priority_fee_distribution_program_id: String,

    /// Mainnet GCP server names
    mainnet_gcp_server_names: Vec<String>,
}

impl KobeWriterService {
    /// Initialize [`KobeWriterService`]
    ///
    /// [`KobeWriterService`] uses multiple managers to fetch information such as validators, stake
    /// pool. so initialize these services in this function.
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        mongo_connection_uri: &str,
        cluster: Cluster,
        rpc_url: String,
        tip_distribution_program_id: String,
        priority_fee_distribution_program_id: String,
        mainnet_gcp_server_names: Vec<String>,
        steward_config: Pubkey,
        bam_api_base_url: Option<String>,
    ) -> Result<Self> {
        let mongodb_client = db::setup_mongo_client(mongo_connection_uri).await?;

        let stake_pool_address = stake_pool_manager::resolve_stake_pool_address(&cluster)?;

        let db = mongodb_client.database(DATABASE_NAME);
        let rpc_client = RpcClient::new_with_timeout_and_commitment(
            rpc_url,
            Duration::from_secs(60),
            CommitmentConfig::confirmed(),
        );
        let rpc_client = Arc::new(rpc_client);

        let stake_pool = get_stake_pool(&rpc_client, &stake_pool_address).await?;

        // Calls a blocking reqwest method when initializing the client
        let validators_app_client = tokio::task::spawn_blocking(move || {
            ValidatorsAppClient::new_with_cluster(cluster)
                .expect("Could not initialize Validators App client")
        })
        .await
        .expect("Failed to initialize Validators App client");
        let validators_app_client = Arc::new(validators_app_client);

        let stake_pool_manager = StakePoolManager::new(
            rpc_client.clone(),
            validators_app_client.clone(),
            bam_api_base_url,
            cluster,
            steward_config,
        );

        let bam_boost_manager =
            BamBoostManager::new(rpc_client.clone(), validators_app_client, cluster);

        Ok(Self {
            db,
            stake_pool_manager,
            bam_boost_manager,
            stake_pool,
            stake_pool_address,
            cluster,
            tip_distribution_program_id,
            priority_fee_distribution_program_id,
            mainnet_gcp_server_names,
        })
    }

    /// A randomized delay until the next BAM snapshot, uniformly in
    /// `BAM_SNAPSHOT_BASE_SECS ± BAM_SNAPSHOT_JITTER_SECS`.
    fn next_bam_snapshot_delay(&self) -> Duration {
        let secs = rand::thread_rng().gen_range(
            (BAM_SNAPSHOT_BASE_SECS - BAM_SNAPSHOT_JITTER_SECS)
                ..=(BAM_SNAPSHOT_BASE_SECS + BAM_SNAPSHOT_JITTER_SECS),
        );
        Duration::from_secs(secs)
    }

    /// Run [`KobeWriterService`] in live mode
    ///
    /// In this mode, the service processes:
    ///
    /// Every 10 min
    /// - Collect validator information from validators app and on-chain, then update and insert
    ///   into DB
    /// - Collect MEV Claim information from on-chain and GCP server, then update into DB
    ///
    /// Every 30 min
    /// - Record one BAM connection snapshot (separate from the validator write so a BAM API
    ///   outage never blocks validator data, and the snapshot cadence is explicit)
    ///
    /// Hourly
    /// - Collect stake pool stats from on-chain, then write into DB
    pub async fn run_live_mode(&self) -> Result<()> {
        let mut next_hourly_update = Instant::now();
        let mut next_bam_snapshot = Instant::now() + self.next_bam_snapshot_delay();

        info!("Starting live mode with 10-minute epoch processing intervals");

        loop {
            let next_run_time = Instant::now() + Duration::from_secs(600); // 10 minutes

            // Process epoch every 10 minutes
            info!("Processing epoch...");
            let epoch = self.process_epoch().await?;
            info!("Epoch processing completed");

            // Record a BAM connection snapshot roughly every 30 minutes (jittered), independent
            // of validator writes. Note the check only runs once per ~10-minute loop tick, so
            // the jitter varies the interval between snapshots rather than giving sub-tick timing.
            if Instant::now() >= next_bam_snapshot {
                info!("Recording BAM connection snapshot for epoch {epoch}");
                match write_bam_snapshot_info(&self.db, &self.stake_pool_manager, epoch).await {
                    Ok(_) => {
                        datapoint_info!("bam_snapshot_written", ("success", 1, i64), "cluster" => self.cluster.to_string());
                    }
                    Err(e) => {
                        error!("Writing BAM snapshot failed. Error: {e:?}");
                        datapoint_info!("bam_snapshot_written", ("success", 0, i64), "cluster" => self.cluster.to_string());
                    }
                }

                next_bam_snapshot = Instant::now() + self.next_bam_snapshot_delay();
            }

            // Check if it's time for the hourly update
            if Instant::now() >= next_hourly_update {
                info!("Performing hourly stake pool stats update");
                match write_stake_pool_info(
                    &self.db,
                    &self.stake_pool_manager,
                    &self.stake_pool_address,
                )
                .await
                {
                    Ok(_) => {
                        info!("Stake pool stats written successfully");
                        datapoint_info!("stake_pool_stats_written", ("success", 1, i64), "cluster" => self.cluster.to_string());
                    }
                    Err(e) => {
                        error!("Writing stake pool stats failed. Error: {e:?}");
                        datapoint_info!("stake_pool_stats_written", ("success", 0, i64), "cluster" => self.cluster.to_string());
                    }
                }

                match self.bam_boost_manager.write_bam_boost_info(&self.db).await {
                    Ok(_) => {
                        info!("BAM Boost stats written successfully");
                        datapoint_info!("bam_boost_stats_written", ("success", 1, i64), "cluster" => self.cluster.to_string());
                    }
                    Err(e) => {
                        error!("Writing BAM Boost stats failed. Error: {e:?}");
                        datapoint_info!("bam_boost_stats_written", ("success", 0, i64), "cluster" => self.cluster.to_string());
                    }
                }
                next_hourly_update = Instant::now() + Duration::from_secs(3600);
                // Update next hourly update time
            }

            info!("Sleeping for 10 minutes until next epoch processing");
            sleep_until(next_run_time).await;
        }
    }

    /// Run [`KobeWriterService`] in backfill mode
    ///
    /// In this mode, the service processes backfilling the mev claims info of specific epoch.
    pub async fn run_backfill_mode(&self, backfill_epoch: u64) -> Result<()> {
        write_mev_claims_info(
            &self.db,
            backfill_epoch,
            &self.tip_distribution_program_id,
            &self.priority_fee_distribution_program_id,
            &self.mainnet_gcp_server_names,
        )
        .await
        .map_err(|e| {
            error!("Writing MEV claims failed. Error: {e:?}");
            e
        })?;
        Ok(())
    }

    /// Process the epoch by writing validator info and MEV claims info to the database.
    ///
    /// Returns the current epoch so the caller can reuse it (e.g. for the BAM snapshot) without
    /// a second RPC round-trip.
    async fn process_epoch(&self) -> Result<u64> {
        let epoch = rpc_utils::retry_get_epoch_info(&self.stake_pool_manager.rpc_client).await?;

        match write_validator_info(
            &self.db,
            &self.stake_pool_manager,
            epoch,
            &self.stake_pool.validator_list,
        )
        .await
        {
            Ok(_) => {
                datapoint_info!("validator_stats_written", ("success", 1, i64), "cluster" => self.cluster.to_string());
            }
            Err(e) => {
                datapoint_info!("validator_stats_written", ("success", 0, i64), "cluster" => self.cluster.to_string());
                error!("Writing validator info failed: {e:?}");
            }
        }

        // On epoch boundaries, this function may fail, so we log the error and continue
        match write_mev_claims_info(
            &self.db,
            epoch - 1,
            &self.tip_distribution_program_id,
            &self.priority_fee_distribution_program_id,
            &self.mainnet_gcp_server_names,
        )
        .await
        {
            Ok(_) => {
                datapoint_info!("mev_claims_written", ("success", 1, i64), "cluster" => self.cluster.to_string());
            }
            Err(e) => {
                datapoint_info!("mev_claims_written", ("success", 0, i64), "cluster" => self.cluster.to_string());
                error!("Writing MEV claims failed: {e:?}");
            }
        }

        Ok(epoch)
    }
}
