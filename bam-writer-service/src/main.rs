use std::time::Duration;

use bam_api_client::client::BamApiClient;
use clap::{Parser, Subcommand};
use kobe_bam_writer_service::BamWriterService;
use kobe_core::{
    db_models::bam_epoch_metric::{BamEpochMetric, BamEpochMetricStore},
    validators_app::Cluster,
};
use log::{error, info};
use mongodb::{Client, Collection};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
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

    /// Whether to dry run before writing to db
    #[clap(long, env, action)]
    dry_run: bool,

    /// Cluster name for metrics
    #[clap(long, env, default_value = "mainnet")]
    cluster_name: String,
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
    // Connect to MongoDB
    let client = Client::with_uri_str(&args.mongo_connection_uri)
        .await
        .expect("Failed to connect to MongoDB");
    let db = client.database(&args.mongo_db_name);

    let bam_epoch_metric_collection: Collection<BamEpochMetric> =
        db.collection(BamEpochMetricStore::COLLECTION);
    let bam_epoch_metric_store = BamEpochMetricStore::new(bam_epoch_metric_collection);

    // Connect to RPC node
    let rpc_client = RpcClient::new_with_timeout_and_commitment(
        args.rpc_url,
        Duration::from_secs(20),
        CommitmentConfig::finalized(),
    );

    let bam_api_config = bam_api_client::config::Config::custom(args.bam_api_base_url);
    let bam_api_client = BamApiClient::new(bam_api_config);

    let cluster = Cluster::get_cluster(&args.cluster_name)?;

    let bam_writer_service = BamWriterService::new(
        args.stake_pool,
        rpc_client,
        bam_api_client,
        bam_epoch_metric_store,
        cluster,
    );

    match args.command {
        Commands::Run => {
            info!("Running BAM writer service");

            // loop { // FIXME
            if let Err(e) = bam_writer_service.run().await {
                error!("Error: {e}");
            }
            // }
        }
    }

    Ok(())
}
