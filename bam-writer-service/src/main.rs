use clap::{Parser, Subcommand};
use kobe_bam_writer_service::BamWriterService;
use log::{error, info};
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

    let bam_writer_service = BamWriterService::new(
        &args.cluster_name,
        &args.mongo_connection_uri,
        &args.mongo_db_name,
        args.stake_pool,
        &args.rpc_url,
        &args.bam_api_base_url,
    )
    .await?;

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
