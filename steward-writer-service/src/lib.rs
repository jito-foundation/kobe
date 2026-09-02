//! Transaction reading and event parsing for the steward writer.
//!
//! These live in a library rather than the binary so the reading path can be
//! driven directly — see `examples/verify_transaction_format.rs`, which runs it
//! against a live RPC.

use std::str::FromStr;

use anchor_client::handle_program_log;
use jito_steward::{
    events::{
        AutoAddValidatorEvent, AutoRemoveValidatorEvent, DecreaseComponents,
        DirectedRebalanceEvent, EpochMaintenanceEvent, InstantUnstakeComponents, RebalanceEvent,
        ScoreComponents, StateTransition,
    },
    score::{InstantUnstakeComponentsV3, ScoreComponentsV5},
};
use kobe_core::db_models::steward_events::StewardEvent;
use log::error;
use solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature;
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    transaction::{TransactionError, TransactionVersion},
};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage,
};

/// Pairs fetched transactions back up with the signature statuses they came from.
///
/// `retry_get_transactions` returns one entry per requested signature in request
/// order, so this is a positional zip. The equality check guards against
/// attributing a transaction's events to the wrong signature if that ever stops
/// holding.
pub fn pair_with_statuses(
    signatures: &[RpcConfirmedTransactionStatusWithSignature],
    transactions: Vec<(Signature, EncodedConfirmedTransactionWithStatusMeta)>,
) -> Vec<(
    RpcConfirmedTransactionStatusWithSignature,
    EncodedConfirmedTransactionWithStatusMeta,
)> {
    signatures
        .iter()
        .zip(transactions)
        .filter_map(|(status, (signature, tx))| {
            if status.signature != signature.to_string() {
                error!(
                    "Signature mismatch: requested {}, RPC returned {signature}",
                    status.signature
                );
                return None;
            }
            Some((status.clone(), tx))
        })
        .collect()
}

/// Human-readable transaction version, for diagnostics.
pub fn describe_version(version: Option<&TransactionVersion>) -> String {
    match version {
        Some(TransactionVersion::Number(n)) => format!("v{n}"),
        Some(TransactionVersion::Legacy(_)) => "legacy".to_string(),
        None => "unknown-version".to_string(),
    }
}

/// Reads the fee payer out of an RPC-parsed transaction message.
///
/// The first account key is the fee payer in every transaction version. Taking
/// it from the node's parsed output rather than deserializing the message
/// ourselves is what lets this keep working for versions the pinned SDK has no
/// decoder for — see the encoding choice in `kobe_core::rpc_utils`.
pub fn fee_payer(transaction: &EncodedTransaction) -> Option<Pubkey> {
    let first_key = match transaction {
        EncodedTransaction::Json(ui_transaction) => match &ui_transaction.message {
            UiMessage::Raw(message) => message.account_keys.first().cloned(),
            UiMessage::Parsed(message) => {
                message.account_keys.first().map(|key| key.pubkey.clone())
            }
        },
        // A binary encoding was requested somewhere; we can't parse it here
        // without a version-aware decoder.
        EncodedTransaction::LegacyBinary(_)
        | EncodedTransaction::Binary(_, _)
        | EncodedTransaction::Accounts(_) => None,
    }?;

    Pubkey::from_str(&first_key).ok()
}

#[allow(clippy::too_many_arguments)]
pub async fn parse_log(
    log: String,
    signature: &Signature,
    instruction_idx: u32,
    signer: &Pubkey,
    stake_pool: &Pubkey,
    timestamp: Option<i64>,
    transaction_err: Option<TransactionError>,
    epoch: u64,
    slot: u64,
) -> Result<Option<StewardEvent>, Box<dyn std::error::Error>> {
    // Parse the log
    let program = jito_steward::id().to_string();
    let tx_error = transaction_err.map(|e| e.to_string());

    // DecreaseComponents
    if let Ok((Some(event), _, _)) = handle_program_log::<DecreaseComponents>(&program, &log) {
        let steward_event = StewardEvent::from_decrease_components(
            event,
            signature,
            instruction_idx,
            tx_error,
            epoch,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // InstantUnstakeComponents
    if let Ok((Some(event), _, _)) = handle_program_log::<InstantUnstakeComponents>(&program, &log)
    {
        let steward_event = StewardEvent::from_instant_unstake_components(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // InstantUnstakeComponentsV3
    if let Ok((Some(event), _, _)) =
        handle_program_log::<InstantUnstakeComponentsV3>(&program, &log)
    {
        let steward_event = StewardEvent::from_instant_unstake_components_v3(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // RebalanceEvent
    if let Ok((Some(event), _, _)) = handle_program_log::<RebalanceEvent>(&program, &log) {
        let steward_event = StewardEvent::from_rebalance_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // DirectedRebalanceEvent
    if let Ok((Some(event), _, _)) = handle_program_log::<DirectedRebalanceEvent>(&program, &log) {
        let steward_event = StewardEvent::from_directed_rebalance_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // ScoreComponents
    if let Ok((Some(event), _, _)) = handle_program_log::<ScoreComponents>(&program, &log) {
        let steward_event = StewardEvent::from_score_components(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // ScoreComponentsV5
    if let Ok((Some(event), _, _)) = handle_program_log::<ScoreComponentsV5>(&program, &log) {
        let steward_event = StewardEvent::from_score_components_v5(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // StateTransition
    if let Ok((Some(event), _, _)) = handle_program_log::<StateTransition>(&program, &log) {
        let steward_event = StewardEvent::from_state_transition(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // AutoRemoveValidatorEvent
    if let Ok((Some(event), _, _)) =
        handle_program_log::<AutoRemoveValidatorEvent>(&program.to_string(), &log)
    {
        let steward_event = StewardEvent::from_auto_remove_validator_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            epoch,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // AutoAddValidatorEvent
    if let Ok((Some(event), _, _)) =
        handle_program_log::<AutoAddValidatorEvent>(&program.to_string(), &log)
    {
        let steward_event = StewardEvent::from_auto_add_validator_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            epoch,
            slot,
        );
        return Ok(Some(steward_event));
    }

    // EpochMaintenanceEvent
    if let Ok((Some(event), _, _)) =
        handle_program_log::<EpochMaintenanceEvent>(&program.to_string(), &log)
    {
        let steward_event = StewardEvent::from_epoch_maintenance_event(
            event,
            signature,
            instruction_idx,
            tx_error,
            signer,
            stake_pool,
            timestamp,
            epoch,
            slot,
        );
        return Ok(Some(steward_event));
    }

    Ok(None)
}

pub fn get_epoch_from_slot(slot: u64) -> u64 {
    // Calculate the epoch from the slot

    slot / 432_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_transaction_status::EncodedTransactionWithStatusMeta;

    const FEE_PAYER: &str = "6WS1UtWtyeJHrsGbcARDPuLoXQeQTuFJHnsL2h1yc9CB";
    const OTHER_KEY: &str = "SysvarC1ock11111111111111111111111111111111";

    /// A `getTransaction` response as an Agave v4.2 node renders a v1
    /// transaction: `version: 1`, and a `transactionConfig` field on the
    /// message that this SDK's `UiRawMessage` has no field for.
    fn v1_transaction_json() -> String {
        format!(
            r#"{{
                "transaction": {{
                    "signatures": ["4HVYbFHkGwPjsHo3jNTfBnhJKzHrMg1S5g8vCsWpMHQFmDGKcNEqzP2M8j1YFwGZJnVw8UzKRZKtQPk2ZTgNYQwr"],
                    "message": {{
                        "header": {{
                            "numRequiredSignatures": 1,
                            "numReadonlySignedAccounts": 0,
                            "numReadonlyUnsignedAccounts": 1
                        }},
                        "accountKeys": ["{FEE_PAYER}", "{OTHER_KEY}"],
                        "recentBlockhash": "EkSnNWid2cvwEVnVx9aBqawnmiCNiDgp3gUdkDPTKN1N",
                        "instructions": [
                            {{"programIdIndex": 1, "accounts": [0], "data": "3Bxs"}}
                        ],
                        "transactionConfig": {{
                            "computeUnitLimit": 200000,
                            "loadedAccountsDataSizeLimit": 65536,
                            "feeLamports": 5000
                        }}
                    }}
                }},
                "meta": {{
                    "err": null,
                    "status": {{"Ok": null}},
                    "fee": 5000,
                    "preBalances": [1000000, 0],
                    "postBalances": [995000, 0],
                    "logMessages": ["Program log: hello"]
                }},
                "version": 1
            }}"#
        )
    }

    /// The whole v1 mitigation rests on this: an unknown `transactionConfig`
    /// field must not fail deserialization, or every v1 transaction becomes an
    /// RPC error rather than a readable record.
    #[test]
    fn v1_response_deserializes_under_pinned_sdk() {
        let encoded: EncodedTransactionWithStatusMeta =
            serde_json::from_str(&v1_transaction_json()).expect("v1 response should deserialize");

        assert_eq!(encoded.version, Some(TransactionVersion::Number(1)));
        assert_eq!(describe_version(encoded.version.as_ref()), "v1");
    }

    #[test]
    fn fee_payer_reads_first_account_key_of_v1_transaction() {
        let encoded: EncodedTransactionWithStatusMeta =
            serde_json::from_str(&v1_transaction_json()).unwrap();

        assert_eq!(
            fee_payer(&encoded.transaction),
            Some(Pubkey::from_str(FEE_PAYER).unwrap())
        );
    }

    /// A binary encoding yields no fee payer here by design: parsing it needs a
    /// version-aware `bincode` decoder, which is the thing this SDK lacks. The
    /// matching guard on the request side lives in `kobe_core::rpc_utils`.
    #[test]
    fn fee_payer_rejects_binary_encoding() {
        let binary = EncodedTransaction::LegacyBinary("not parseable here".to_string());

        assert_eq!(fee_payer(&binary), None);
    }

    /// The five Anchor event logs emitted by a real `ComputeScore` transaction,
    /// mainnet signature
    /// `3AaEZ6PUYUmNH8tFbx3tHUs6buJhY3KhhNtri6U1Rm4xGo17ct2M6nVyCEdXqn7DoVjakgYBwoaAAg9EDAY5PmjB`
    /// at slot 440863635. Verbatim, so this covers the real on-chain event
    /// layout rather than one we constructed to match the decoder.
    const COMPUTE_SCORE_EVENT_LOGS: [&str; 5] = [
        "Program data: HqAPeIvXzYIAAAAAAAAAAK//PBgAoIxfBegDDAAAAK//PAABAQEAAAEBAQaIjdQdpdfhgZtLvBcd/40lJ0Xw87GYrNLoZWOCWwnL/APoA/AD//8AAAAAAAAAAN4DBfADBfADAAD//wEB",
        "Program data: HqAPeIvXzYIAAAAAAAAAAI4APRgAoIxfBegDDAAAAI4APQABAQEAAQEBAfY3xpialume/Kul7cD7zTtHMU62qGkHLR3yAm8ydlrv/APoA/AD//8AAAAAAAAAAN4DBfADBfADAAD//wEB",
        "Program data: HqAPeIvXzYIAAAAAAAAAAF6lKBAAcJRfBfQBCAAAAF6lKAABAQEAAAEBAeQpUoVdDp1KFUpvB84QbrDAG6KlLw/zMKtIqUEytqoy/AP0AfQD//8AAAAAAAAAAN4DBfQDBfQDAAD//wEB",
        "Program data: HqAPeIvXzYIAAAAAAAAAALxiGQoAQJxfBQAABQAAALxiGQABAQEAAQEBAb7uYNbiR9EumO+5zbWeuSxnHpx9d/vAP8EO42iowV/1/AMAAPwD//8AAAAAAAAAAN4DBfcDBfcDAAD//wEB",
        "Program data: agl496lqzun8AwAAAAAAAJMLRxoAAAAADQAAAENvbXB1dGVTY29yZXMSAAAAQ29tcHV0ZURlbGVnYXRpb25z",
    ];

    /// Answers the question the encoding change raises end to end: a real
    /// `ComputeScore` transaction still decodes into the events the writer
    /// stores. `parse_log` reads `meta.log_messages`, which the `Json` encoding
    /// leaves untouched — this pins that.
    #[tokio::test]
    async fn decodes_events_from_real_compute_score_transaction() {
        let signature = Signature::from_str(
            "3AaEZ6PUYUmNH8tFbx3tHUs6buJhY3KhhNtri6U1Rm4xGo17ct2M6nVyCEdXqn7DoVjakgYBwoaAAg9EDAY5PmjB",
        )
        .unwrap();
        let signer = Pubkey::from_str("CRnkKQTxctQ7LHVN3yssdgJyEksBJeBrDdwZAxBtsJoZ").unwrap();
        let stake_pool = Pubkey::new_unique();
        let slot = 440_863_635;

        let mut events = Vec::new();
        for log in COMPUTE_SCORE_EVENT_LOGS {
            let parsed = parse_log(
                log.to_string(),
                &signature,
                0,
                &signer,
                &stake_pool,
                Some(1_787_380_325),
                None,
                get_epoch_from_slot(slot),
                slot,
            )
            .await
            .expect("parse_log should not error");

            if let Some(event) = parsed {
                events.push(event);
            }
        }

        let event_types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        assert_eq!(
            event_types,
            vec![
                "ScoreComponentsV5",
                "ScoreComponentsV5",
                "ScoreComponentsV5",
                "ScoreComponentsV5",
                "StateTransition",
            ],
        );

        // The score events carry the validator they scored; the state
        // transition is pool-wide and carries none.
        assert!(events[..4].iter().all(|e| e.vote_account.is_some()),);
        assert!(events.iter().all(|e| e.signer == signer.to_string()));
        assert!(events.iter().all(|e| e.slot == slot));
    }
}
