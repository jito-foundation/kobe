use std::time::Duration;

use clap::{Parser, Subcommand};
use kobe_bam_writer_service::BamWriterService;
use log::{error, info};
use solana_client::nonblocking::rpc_client::RpcClient;
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

    /// Kobe api base url
    #[clap(long, env)]
    kobe_api_base_url: String,

    /// Stake pool address
    #[clap(
        long,
        env,
        default_value = "Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb"
    )]
    stake_pool: Pubkey,

    /// Cluster name for metrics
    #[clap(long, env, default_value = "mainnet")]
    cluster_name: String,

    /// Epoch progress threshold to trigger (0.0-1.0, default 0.9 for 90%)
    #[clap(long, env, default_value = "0.9")]
    epoch_progress_threshold: f64,

    /// Poll interval in seconds
    #[clap(long, env, default_value = "60")]
    poll_interval_secs: u64,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run bam writer service
    Run,
}

async fn wait_for_epoch_threshold(
    rpc_client: &RpcClient,
    threshold: f64,
    poll_interval: Duration,
) -> anyhow::Result<u64> {
    loop {
        let epoch_info = rpc_client.get_epoch_info().await?;
        let progress = epoch_info.slot_index as f64 / epoch_info.slots_in_epoch as f64;

        info!(
            "Epoch: {}, Progress: {:.2}% ({}/{})",
            epoch_info.epoch,
            progress * 100.0,
            epoch_info.slot_index,
            epoch_info.slots_in_epoch
        );

        if progress >= threshold {
            info!(
                "Reached {}% of epoch {}",
                threshold * 100.0,
                epoch_info.epoch
            );
            return Ok(epoch_info.epoch);
        }

        tokio::time::sleep(poll_interval).await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();

    let rpc_client = RpcClient::new(args.rpc_url.clone());
    let poll_interval = Duration::from_secs(args.poll_interval_secs);

    let bam_writer_service = BamWriterService::new(
        &args.mongo_connection_uri,
        &args.mongo_db_name,
        args.stake_pool,
        &args.rpc_url,
        &args.bam_api_base_url,
        &args.kobe_api_base_url,
    )
    .await?;

    match args.command {
        Commands::Run => {
            info!("Running BAM writer service");
            let mut last_processed_epoch: Option<u64> = None;

            loop {
                match wait_for_epoch_threshold(
                    &rpc_client,
                    args.epoch_progress_threshold,
                    poll_interval,
                )
                .await
                {
                    Ok(current_epoch) => {
                        // Only run if we haven't processed this epoch yet
                        if last_processed_epoch != Some(current_epoch) {
                            info!("Processing epoch {current_epoch}");

                            match bam_writer_service.run().await {
                                Ok(()) => {
                                    info!("Successfully processed epoch {current_epoch}");
                                    last_processed_epoch = Some(current_epoch);

                                    datapoint_info!(
                                        "bam-writer-service-stats",
                                        ("epoch", current_epoch, i64),
                                        ("success", 1, i64),
                                        "cluster" => args.cluster_name,
                                    );
                                }
                                Err(e) => {
                                    error!("Error processing epoch {current_epoch}: {e}");

                                    datapoint_info!(
                                        "bam-writer-service-stats",
                                        ("epoch", current_epoch, i64),
                                        ("success", 0, i64),
                                        "cluster" => args.cluster_name,
                                    );
                                }
                            }
                        }

                        // Sleep a bit before checking again
                        tokio::time::sleep(poll_interval).await;
                    }
                    Err(e) => {
                        error!("Error checking epoch info: {}", e);

                        tokio::time::sleep(poll_interval).await;
                    }
                }
            }
        }
    }
}
