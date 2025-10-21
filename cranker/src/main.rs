//! main crank loop, to be evaluated approximately once every epoch

use std::time::Duration;

use anyhow::anyhow;
use backon::{ExponentialBuilder, Retryable};
use bincode::deserialize;
use clap::Parser;
use env_logger::{Builder, Target};
use kobe_core::validators_app::Cluster;
use log::*;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_epoch_rewards::EpochRewards;
use solana_metrics::set_host_id;
use solana_sdk::{clock::Epoch, commitment_config::CommitmentConfig, sysvar::SysvarId};
use tokio::{runtime::Runtime, time::sleep as tokio_sleep};

use crate::{
    args::Args,
    metrics::StakePoolMetrics,
    utils::{
        fetch_epoch, get_signer, parallel_execute_stake_pool_update, post_slack_message,
        wait_for_next_epoch, Config,
    },
};

mod args;
mod metrics;
mod utils;

/// Performs all the actions needed per epoch to manage the stake pool
///
/// - Update stake pool to current epoch (even on dry run)
async fn update_stake_pool(config: &Config, epoch: Epoch) -> anyhow::Result<()> {
    loop {
        let epoch_rewards_data = config
            .rpc_client
            .get_account_data(&EpochRewards::id())
            .await?;
        let epoch_rewards: EpochRewards = deserialize(&epoch_rewards_data)?;

        if epoch_rewards.active {
            tokio::time::sleep(Duration::from_secs(60)).await;
            continue;
        }

        let slack_message =
            match parallel_execute_stake_pool_update(config, epoch, true, false).await {
                Ok(()) => "Cranker has successfully run Stake Pool Update",
                Err(e) => {
                    error!("Cranker failed to update, {e:?}");
                    "Cranker failed to update. Please manually run stake pool update"
                }
            };

        post_slack_message(config.slack_api_token.clone(), slack_message)
            .map_err(|e| anyhow!("Slack message failed to post, {e:?}"))?;

        return Ok(());
    }
}

fn main() -> anyhow::Result<()> {
    let mut builder = Builder::new();
    builder.target(Target::Stdout).parse_default_env();
    let logger = sentry_log::SentryLogger::with_dest(builder.build());
    log::set_boxed_logger(Box::new(logger)).unwrap();
    log::set_max_level(LevelFilter::Debug);

    let args = Args::parse();

    let cli_config = if let Some(config_file) = args.get_config_file() {
        solana_cli_config::Config::load(&config_file).unwrap_or_default()
    } else {
        solana_cli_config::Config::default()
    };

    let _guard = sentry::init((
        args.sentry_api_url.as_str(),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            ..Default::default()
        },
    ));
    info!("Sentry guard initialized");

    let cluster = args.get_cluster();

    let hostname_cmd = std::process::Command::new("hostname")
        .output()
        .expect("Failed to execute hostname command");

    let hostname = String::from_utf8_lossy(&hostname_cmd.stdout)
        .trim()
        .to_string();

    // Set up host id for metrics
    set_host_id(format!(
        "kobe-cranker-{}-{}-{}",
        args.region,
        args.get_cluster(),
        hostname
    ));

    // if a valid pool address is provided via CLI, then use it. Otherwise, use defaults.
    let stake_pool_address = args.get_stake_pool_address();
    info!("pool address at {stake_pool_address:#?}");

    let json_rpc_url = args.get_json_rpc_url();

    let rpc_client =
        RpcClient::new_with_commitment(json_rpc_url.clone(), CommitmentConfig::confirmed());
    let config: Config = {
        let fee_payer = get_signer(args.fee_payer.as_deref(), &cli_config.keypair_path);

        Config {
            rpc_client,
            cluster: Cluster::get_cluster(&args.network).expect("Failed to get cluster"),
            fee_payer,
            stake_pool_address,
            dry_run: args.dry_run,
            simulate: args.simulate,
            slack_api_token: args.slack_api_token,
        }
    };

    let runtime = Runtime::new().expect("Tokio runtime failed to create");

    runtime.block_on(async {
        let mut epoch = (|| fetch_epoch(&config.rpc_client))
            .retry(ExponentialBuilder::default())
            .await
            .expect("Function panicked fetching epoch info while waiting for next epoch")
            .epoch;
        if config.dry_run {
            // Don't need to loop if just dry running
            if let Err(e) = update_stake_pool(&config, epoch).await {
                error!("{e}");
            }
        } else {
            // Periodically report metrics every minute
            runtime.spawn(async move {
                loop {
                    tokio_sleep(Duration::from_secs(60)).await;
                    match StakePoolMetrics::new(
                        json_rpc_url.clone(),
                        stake_pool_address,
                        cluster.to_string(),
                    )
                    .report_metrics()
                    .await
                    {
                        Ok(_) => {}
                        Err(e) => {
                            error!("failed to report metrics with error {e:#?}");
                        }
                    }
                }
            });
            loop {
                if let Err(e) = update_stake_pool(&config, epoch).await {
                    error!("{e}");
                }
                epoch = wait_for_next_epoch(&config.rpc_client)
                    .await
                    .expect("Function panicked fetching epoch info while waiting for next epoch");
            }
        }
    });

    Ok(())
}
