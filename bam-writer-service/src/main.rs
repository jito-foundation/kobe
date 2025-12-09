use std::{collections::HashSet, sync::Arc, time::Duration};

use clap::{Parser, Subcommand};
use kobe_bam_writer_service::BamWriterService;
use log::{error, info};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_metrics::datapoint_info;
use solana_pubkey::Pubkey;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    command: Commands,

    /// Mongo connection URI.
    #[clap(long, env)]
    mongo_connection_uri: String,

    /// Mongo database name.
    #[clap(long, env)]
    mongo_db_name: String,

    /// RPC URL.
    #[clap(long, env)]
    rpc_url: String,

    /// BAM api base url
    #[clap(long, env)]
    bam_api_base_url: String,

    /// Stake pool address
    #[clap(
        long,
        env,
        default_value = "Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb"
    )]
    stake_pool: Pubkey,

    /// Steward config account address
    #[clap(
        long,
        env,
        default_value = "jitoVjT9jRUyeXHzvCwzPgHj7yWNRhLcUoXtes4wtjv"
    )]
    steward_config: Pubkey,

    /// Cluster name for metrics
    #[clap(long, env, default_value = "mainnet")]
    cluster_name: String,

    /// Epoch progress thresholds to trigger (0.0-1.0, default 50%, 75%, 90%)
    #[clap(long, env, value_delimiter = ',', default_value = "0.5,0.75,0.9")]
    epoch_progress_thresholds: Vec<f64>,

    /// Poll interval in seconds
    #[clap(long, env, default_value = "60")]
    poll_interval_secs: u64,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run bam writer service
    Run,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();

    let rpc_client = RpcClient::new_with_timeout_and_commitment(
        args.rpc_url.to_string(),
        Duration::from_secs(20),
        CommitmentConfig::finalized(),
    );
    let rpc_client = Arc::new(rpc_client);

    let poll_interval = Duration::from_secs(args.poll_interval_secs);

    let bam_writer_service = BamWriterService::new(
        &args.cluster_name,
        &args.mongo_connection_uri,
        &args.mongo_db_name,
        args.stake_pool,
        args.steward_config,
        rpc_client.clone(),
        &args.bam_api_base_url,
    )
    .await?;

    match args.command {
        Commands::Run => {
            info!("Running BAM writer service");
            let mut last_processed_epoch: Option<u64> = None;
            let mut thresholds_hit = HashSet::new();

            loop {
                let epoch_info = rpc_client.get_epoch_info().await?;
                let current_epoch = epoch_info.epoch;
                let progress = epoch_info.slot_index as f64 / epoch_info.slots_in_epoch as f64;

                if last_processed_epoch != Some(current_epoch) {
                    thresholds_hit.clear();
                    last_processed_epoch = Some(current_epoch);
                }

                for (idx, &threshold) in args.epoch_progress_thresholds.iter().enumerate() {
                    if progress >= threshold && !thresholds_hit.contains(&idx) {
                        let threshold_pct = threshold * 100.0;

                        info!("Reached {threshold_pct:.0}% threshold for epoch {current_epoch}");

                        match bam_writer_service.run().await {
                            Ok(()) => {
                                info!("Successfully processed at {threshold_pct:.0}% of epoch {current_epoch}");
                                thresholds_hit.insert(idx);

                                datapoint_info!(
                                    "bam-writer-service-stats",
                                    ("epoch", current_epoch, i64),
                                    ("threshold_pct", threshold_pct as i64, i64),
                                    ("success", 1, i64),
                                    "cluster" => args.cluster_name,
                                );
                            }
                            Err(e) => {
                                error!(
                                    "Error processing at {threshold_pct:.0}% of epoch {current_epoch}: {e}"
                                );

                                datapoint_info!(
                                    "bam-writer-service-stats",
                                    ("epoch", current_epoch, i64),
                                    ("success", 0, i64),
                                    "cluster" => args.cluster_name,
                                );
                            }
                        }
                    }
                }

                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}
