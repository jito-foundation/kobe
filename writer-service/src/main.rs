use clap::{Parser, Subcommand};
use env_logger::{Builder, Target};
use kobe_core::validators_app::Cluster;
use kobe_writer_service::{result::Result, KobeWriterService};
use log::{error, info, set_boxed_logger, set_max_level, LevelFilter};
use solana_clap_utils::input_validators::is_url_or_moniker;
use solana_metrics::set_host_id;
use solana_pubkey::Pubkey;
use tokio::runtime::Runtime;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// RPC url
    #[arg(long, env)]
    rpc_url: String,

    /// Mongo connection URI.
    #[arg(long, env)]
    mongo_connection_uri: String,

    /// Mongo database name.
    #[arg(long, env)]
    mongo_db_name: String,

    /// Solana cluster e.g. testnet, mainnet, devnet
    #[arg(long, short, env, default_value_t=String::from("testnet"))]
    solana_cluster: String,

    /// Whether to create the index
    #[arg(long)]
    create_index: bool,

    /// Sentry api url
    #[arg(long, env)]
    sentry_api_url: String,

    // Whether to start running a live loop or single epoch backfill
    #[command(subcommand)]
    command: Commands,

    /// Tip distribution program id
    #[arg(
        long,
        env,
        default_value = "4R3gSG8BpU4t19KYj8CfnbtRpnT8gtk4dvTHxVRwc2r7"
    )]
    tip_distribution_program_id: String,

    /// Priority fee distribution program id
    #[arg(
        long,
        env,
        default_value = "Priority6weCZ5HwDn29NxLFpb7TDp2iLZ6XKc5e8d3"
    )]
    priority_fee_distribution_program_id: String,

    /// Jito steward program id
    #[arg(
        long,
        env,
        default_value = "Stewardf95sJbmtcZsyagb2dg4Mo8eVQho8gpECvLx8"
    )]
    jito_steward_program_id: Pubkey,

    /// Steward config pubkey
    #[arg(
        long,
        env,
        default_value = "jitoVjT9jRUyeXHzvCwzPgHj7yWNRhLcUoXtes4wtjv"
    )]
    steward_config_pubkey: Pubkey,

    /// Mainnet gcp server names
    #[arg(long, env, value_delimiter = ',')]
    mainnet_gcp_server_names: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    Live,
    Backfill(BackfillArgs),
}

#[derive(Parser)]
struct BackfillArgs {
    epoch: u64,
}

fn init_logger(cluster: &Cluster) -> Result<()> {
    let mut builder = Builder::new();
    builder.target(Target::Stdout).parse_default_env();
    let logger = sentry_log::SentryLogger::with_dest(builder.build());
    set_max_level(LevelFilter::Debug);
    set_host_id(format!("kobe_db_writer_{cluster}"));
    Ok(set_boxed_logger(Box::new(logger))?)
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args: Args = Args::parse();
    let cluster = Cluster::get_cluster(&args.solana_cluster)?;
    init_logger(&cluster)?;

    // Set up panic alerting via Sentry
    let _guard = sentry::init((
        args.sentry_api_url.clone(),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            ..Default::default()
        },
    ));
    info!("Sentry guard initialized");

    is_url_or_moniker(args.solana_cluster.clone()).expect("Cluster arg malformed!");

    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let kobe_service = KobeWriterService::new(
            &args.mongo_connection_uri,
            cluster,
            args.rpc_url,
            args.tip_distribution_program_id,
            args.priority_fee_distribution_program_id,
            args.mainnet_gcp_server_names,
            args.jito_steward_program_id,
            args.steward_config_pubkey,
        )
        .await
        .expect("Failed to initialize KobeWriterService");

        info!(
            "Starting db writer svc, monitoring pool {:#?} on cluster {:#?}",
            kobe_service.stake_pool_address, cluster
        );

        let mode = args.command;
        match mode {
            Commands::Live => {
                if let Err(e) = kobe_service.run_live_mode().await {
                    error!("Live mode failed. Error: {e:?}");
                }
            }
            Commands::Backfill(backfill_args) => {
                let BackfillArgs {
                    epoch: backfill_epoch,
                } = backfill_args;

                if let Err(e) = kobe_service.run_backfill_mode(backfill_epoch).await {
                    error!("Backfill failed. Error: {e:?}");
                }
            }
        }
    });

    Ok(())
}
