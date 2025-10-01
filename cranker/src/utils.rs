use std::{collections::HashMap, thread::sleep, time::Duration};

use anyhow::anyhow;
use backon::{ExponentialBuilder, Retryable};
use kobe_core::{constants::CRANKER_UPDATE_CHANNEL, validators_app::Cluster};
use log::*;
use solana_cli_output::display::new_spinner_progress_bar;
use solana_client::{
    client_error::ClientError, nonblocking::rpc_client::RpcClient,
    rpc_config::RpcSendTransactionConfig,
};
use solana_program::{
    clock::Epoch, hash::Hash, instruction::Instruction, message::Message, pubkey::Pubkey,
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    epoch_info::EpochInfo,
    signer::Signer,
    signers::Signers,
    transaction::{Transaction, TransactionError},
};
use solana_transaction_status::UiTransactionEncoding;
use spl_stake_pool::instruction::update_stake_pool;
use spl_stake_pool_cli::client::{get_stake_pool, get_validator_list};
use thiserror::Error as ThisError;

const BATCH_SIZE: usize = 250;
type Error = Box<dyn std::error::Error>;

#[derive(ThisError, Debug)]
pub enum TransactionRetryError {
    #[error("Transactions failed to execute after multiple retries")]
    RetryError,
}

/// Kobe Cranker Config
pub struct Config {
    /// RPC Client
    pub rpc_client: RpcClient,

    /// Cluster (Mainnet-beta, Testnet, Devnet)
    pub cluster: Cluster,

    /// Fee payer
    pub fee_payer: Box<dyn Signer>,

    /// Stake pool address
    pub stake_pool_address: Pubkey,

    /// Dry run mode
    pub dry_run: bool,

    /// Simulate
    pub simulate: bool,

    /// Slack API Token
    pub slack_api_token: Option<String>,
}

pub async fn checked_transaction_with_signers<S: Signers>(
    config: &Config,
    instructions: &[Instruction],
    signers: &S,
) -> Result<Transaction, Error> {
    let recent_blockhash = get_latest_blockhash(&config.rpc_client).await?;
    let message = Message::new_with_blockhash(
        instructions,
        Some(&config.fee_payer.pubkey()),
        &recent_blockhash,
    );
    let transaction = Transaction::new(signers, message, recent_blockhash);
    Ok(transaction)
}

async fn get_latest_blockhash(client: &RpcClient) -> Result<Hash, ClientError> {
    Ok(client
        .get_latest_blockhash_with_commitment(CommitmentConfig::finalized())
        .await?
        .0)
}

async fn get_latest_blockhash_with_retry(client: &RpcClient) -> Result<Hash, ClientError> {
    let mut result;
    for _ in 1..4 {
        result = client
            .get_latest_blockhash_with_commitment(CommitmentConfig::finalized())
            .await;
        if result.is_ok() {
            return Ok(result?.0);
        }
    }
    Ok(client
        .get_latest_blockhash_with_commitment(CommitmentConfig::finalized())
        .await?
        .0)
}

pub fn get_signer(
    keypair_path_override: Option<&str>,
    default_keypair_path: &str,
) -> Box<dyn Signer> {
    let path = keypair_path_override.unwrap_or(default_keypair_path);
    let keypair = solana_sdk::signature::read_keypair_file(path).unwrap_or_else(|e| {
        error!("error reading keypair file {path}: {e}");
        std::process::exit(1);
    });
    Box::new(keypair)
}

pub async fn parallel_execute_transactions(
    mut transactions: Vec<Vec<Instruction>>,
    config: &Config,
    retry: u16,
) -> Result<(), Error> {
    for _ in 1..retry {
        let mut executed_signatures = HashMap::new();
        let mut index = 0usize;
        for transaction_batch in transactions.chunks(BATCH_SIZE) {
            // Convert instructions to transactions in batches and send them all, saving their signatures
            let recent_blockhash = get_latest_blockhash_with_retry(&config.rpc_client).await?;
            let compiled_transactions: Vec<Transaction> = transaction_batch
                .iter()
                .map(|ixs| {
                    Transaction::new_signed_with_payer(
                        ixs,
                        Some(&config.fee_payer.pubkey()),
                        &[config.fee_payer.as_ref()],
                        recent_blockhash,
                    )
                })
                .collect();

            for (i, tx) in compiled_transactions.iter().enumerate() {
                let signature = tx.signatures[0];
                let res = config
                    .rpc_client
                    .send_transaction_with_config(
                        tx,
                        RpcSendTransactionConfig {
                            skip_preflight: true,
                            encoding: Some(UiTransactionEncoding::Base58),
                            ..Default::default()
                        },
                    )
                    .await;

                match res {
                    Ok(_) => {
                        executed_signatures.insert(signature, index + i);
                    }
                    // Filter preflight errors
                    Err(e) => match e.get_transaction_error() {
                        Some(TransactionError::BlockhashNotFound) => {
                            // Need to re-sign and retry this transaction
                            executed_signatures.insert(signature, index + i);
                        }
                        Some(TransactionError::AlreadyProcessed) => {
                            // Signature will be found when confirming
                            executed_signatures.insert(signature, index + i);
                        }
                        Some(_) | None => {
                            // If the transaction failed for any other reason, log the error but don't try to resubmit
                            error!("Transaction failed preflight with err: {e:?}");
                        }
                    },
                }
            }

            index += BATCH_SIZE;
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        tokio::time::sleep(Duration::from_secs(30)).await;

        let confirmation_futures: Vec<_> = executed_signatures
            .clone()
            .into_keys()
            .map(|sig| async move {
                (
                    sig,
                    config
                        .rpc_client
                        .get_signature_status_with_commitment(&sig, CommitmentConfig::confirmed())
                        .await,
                )
            })
            .collect();

        let results = futures::future::join_all(confirmation_futures).await;

        for (sig, result) in results.iter() {
            if matches!(result, Ok(Some(Ok(())))) {
                executed_signatures.remove(sig);
            }
        }

        info!(
            "{} transactions submitted, {} confirmed",
            results.len(),
            results.len() - executed_signatures.len()
        );

        // All have been executed
        if executed_signatures.is_empty() {
            return Ok(());
        }

        // Update instructions to the ones remaining
        transactions = executed_signatures
            .into_values()
            .map(|i| transactions[i].clone())
            .collect();
    }

    Err(Box::new(TransactionRetryError::RetryError))
}

/// CLI "update" command. Formerly `command_update`
///
/// Creates and executes transactions to update validator list balance, update stake pool balance, and
/// delete the removed validator accounts from ValidatorList
pub async fn parallel_execute_stake_pool_update(
    config: &Config,
    epoch: Epoch,
    force: bool,
    no_merge: bool,
) -> anyhow::Result<()> {
    let stake_pool = get_stake_pool(&config.rpc_client, &config.stake_pool_address)
        .await
        .map_err(|e| anyhow!("{e}"))?;

    // to ensure RPC is caught up to new epoch
    let epoch_info = loop {
        let rpc_epoch_info = config.rpc_client.get_epoch_info().await?;
        if rpc_epoch_info.epoch == epoch {
            break rpc_epoch_info;
        }
    };

    if stake_pool.last_update_epoch == epoch_info.epoch {
        if force {
            info!("Update not required, but --force flag specified, so doing it anyway");
        } else {
            return Ok(());
        }
    }

    let validator_list = get_validator_list(&config.rpc_client, &stake_pool.validator_list)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let program_id = match config.cluster {
        Cluster::MainnetBeta | Cluster::Testnet => spl_stake_pool::id(),
        Cluster::Devnet => spl_stake_pool::devnet::id(),
    };

    let (update_list_instructions, final_instructions) = update_stake_pool(
        &program_id,
        &stake_pool,
        &validator_list,
        &config.stake_pool_address,
        no_merge,
    );

    info!(
        "Update list instructions len: {}",
        update_list_instructions.len()
    );
    info!("Final instructions len: {}", final_instructions.len());

    // Priority fee constants
    const INITIAL_PRIORITY_FEE: u64 = 10_000;
    const HIGH_PRIORITY_FEE: u64 = 1_000_000;
    const COMPUTE_UNIT_LIMIT: u32 = 1_400_000;

    // Try to submit with priority fees
    let update_list_transactions_prio_fee: Vec<Vec<Instruction>> = update_list_instructions
        .chunks(2)
        .map(|chunk| {
            let mut instructions = vec![
                ComputeBudgetInstruction::set_compute_unit_price(INITIAL_PRIORITY_FEE),
                ComputeBudgetInstruction::set_compute_unit_limit(600_000),
            ];

            instructions.extend(chunk.to_vec());
            instructions
        })
        .collect();

    if let Err(e) =
        parallel_execute_transactions(update_list_transactions_prio_fee, config, 250).await
    {
        error!("Failed to submit update list transactions with initial priority fee: {e:?}");
        info!("Retrying with higher priority fee");

        let update_list_transactions_prio_fee: Vec<Vec<Instruction>> = update_list_instructions
            .chunks(2)
            .map(|chunk| {
                let mut instructions = vec![
                    ComputeBudgetInstruction::set_compute_unit_price(HIGH_PRIORITY_FEE),
                    ComputeBudgetInstruction::set_compute_unit_limit(600_000),
                ];
                instructions.extend(chunk.to_vec());
                instructions
            })
            .collect();
        parallel_execute_transactions(update_list_transactions_prio_fee, config, 250)
            .await
            .map_err(|e| anyhow!("{e}"))?;
    }

    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_price(INITIAL_PRIORITY_FEE),
        ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
    ];

    instructions.extend(final_instructions.clone());

    let transaction =
        checked_transaction_with_signers(config, &instructions, &[config.fee_payer.as_ref()])
            .await
            .map_err(|e| anyhow!("{e}"))?;

    if let Err(e) = retry_send_transaction(config, &transaction, 250).await {
        error!("Final transaction failed with initial priority fee: {e:?}");
        info!("Retrying with high priority fee");

        let mut final_instructions_prio_fee = vec![
            ComputeBudgetInstruction::set_compute_unit_price(HIGH_PRIORITY_FEE),
            ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
        ];
        final_instructions_prio_fee.extend(final_instructions);
        let transaction = checked_transaction_with_signers(
            config,
            &final_instructions_prio_fee,
            &[config.fee_payer.as_ref()],
        )
        .await
        .map_err(|e| anyhow!("{e}"))?;

        retry_send_transaction(config, &transaction, 250).await?;
    }

    Ok(())
}

pub async fn send_transaction(
    config: &Config,
    transaction: Transaction,
) -> solana_client::client_error::Result<()> {
    let signature = config
        .rpc_client
        .send_transaction_with_config(
            &transaction,
            RpcSendTransactionConfig {
                skip_preflight: true,
                encoding: Some(UiTransactionEncoding::Base58),
                ..Default::default()
            },
        )
        .await?;

    for _ in 0..30 {
        sleep(Duration::from_secs(1));
        let confirmation = config
            .rpc_client
            .get_signature_status_with_commitment(&signature, CommitmentConfig::confirmed())
            .await;
        match confirmation {
            Ok(Some(Ok(()))) => {
                info!("Transaction confirmed: {signature}");
                return Ok(());
            }
            Ok(Some(Err(e))) => {
                error!("Transaction failed: {signature} {e:?}");
                return Err(e.into());
            }
            Ok(None) => {
                // Transaction not yet confirmed
            }
            Err(e) => {
                error!("Failed to get signature status: {e}");
                return Err(e);
            }
        }
    }

    Err(solana_rpc_client_api::client_error::ErrorKind::Custom(
        "Transaction not confirmed".to_string(),
    )
    .into())
}

pub async fn simulate_transaction(
    config: &Config,
    transaction: Transaction,
) -> solana_client::client_error::Result<()> {
    let result = config
        .rpc_client
        .simulate_transaction(&transaction)
        .await?
        .value;
    if result.err.is_some() {
        error!("Err: {:?}", result.err);
        error!("{:?}", result.logs);
    } else {
        info!("Ok");
    }
    Ok(())
}

pub async fn retry_send_transaction(
    config: &Config,
    transaction: &Transaction,
    retry: u16,
) -> solana_client::client_error::Result<()> {
    if config.dry_run {
        return Ok(());
    }
    let mut result: solana_client::client_error::Result<()> = Ok(());
    for _ in 1..retry {
        result = if config.simulate {
            simulate_transaction(config, transaction.clone()).await
        } else {
            send_transaction(config, transaction.clone()).await
        };
        if result.is_ok() {
            return result;
        } else {
            error!("Hit error {result:?}");
        }
    }
    result
}

pub async fn fetch_epoch(rpc_client: &RpcClient) -> anyhow::Result<EpochInfo> {
    let epoch_info = rpc_client
        .get_epoch_info_with_commitment(CommitmentConfig::default())
        .await?;

    Ok(epoch_info)
}

pub async fn wait_for_next_epoch(rpc_client: &RpcClient) -> anyhow::Result<Epoch> {
    let retry_policy = ExponentialBuilder::default()
        .with_max_times(5) // Maximum 5 retry attempts
        .with_min_delay(Duration::from_millis(100))
        .with_max_delay(Duration::from_secs(30));

    let epoch_info = (|| fetch_epoch(rpc_client)).retry(retry_policy).await?;

    let current_epoch = epoch_info.epoch;
    let progress_bar = new_spinner_progress_bar();

    loop {
        sleep(Duration::from_millis(200));
        let epoch_info_result = rpc_client.get_epoch_info().await;
        if epoch_info_result.is_err() {
            continue;
        }
        let epoch_info = epoch_info_result?;
        if epoch_info.epoch > current_epoch {
            return Ok(epoch_info.epoch);
        }
        progress_bar.set_message(format!(
            "Waiting for epoch {} ({} slots remaining)",
            current_epoch + 1,
            epoch_info
                .slots_in_epoch
                .saturating_sub(epoch_info.slot_index),
        ));
    }
}

pub fn post_slack_message(maybe_api_token: Option<String>, message: &str) -> Result<(), Error> {
    if let Some(api_token) = maybe_api_token {
        let mut args = HashMap::new();
        args.insert("text", message);
        args.insert("channel", CRANKER_UPDATE_CHANNEL);
        let client = reqwest::blocking::Client::new();
        client
            .post("https://slack.com/api/chat.postMessage")
            .header("Authorization", format!("Bearer {api_token}"))
            .json(&args)
            .send()?;
    }

    Ok(())
}
