use crate::{constants::*, validators_app::ValidatorsAppResponseEntry};

pub fn performance_score(
    validator_meta: &ValidatorsAppResponseEntry,
    vote_credit_proportion: f64,
    _running_jito: bool,
) -> f64 {
    /* Performance Score as described in https://www.notion.so/jito/Stake-Delegation-Process-0438657ac7634f1fb71af8936fb0980b
    TODO Link Gitbook description */
    let not_delinquent_f64 = if let Some(true) = validator_meta.delinquent {
        0.0
    } else {
        1.0
    };
    let running_jito = 1.0; // jito public API

    let skipped_slots_f64 = validator_meta
        .skipped_slot_percent
        .as_ref()
        .unwrap_or(&String::from("1.0"))
        .parse::<f64>()
        .unwrap();

    not_delinquent_f64
        * running_jito
        * (validator_meta.vote_distance_score.unwrap_or_default() as f64
            + validator_meta.software_version_score.unwrap_or_default() as f64
            + validator_meta.root_distance_score.unwrap_or_default() as f64
            + vote_credit_proportion
            + (1.0 - skipped_slots_f64))
}

pub fn decentralization_score(validator_meta: &ValidatorsAppResponseEntry) -> f64 {
    /* Decentralization Score as described in https://www.notion.so/jito/Stake-Delegation-Process-0438657ac7634f1fb71af8936fb0980b
    TODO Link Gitbook description */

    let dc_concentration_score = validator_meta
        .data_center_concentration_score
        .unwrap_or_default() as f64
        + 2.0;
    let published_information = validator_meta
        .published_information_score
        .unwrap_or_default() as f64;
    // Stake concentration score is -2 if the validator is in the superminority
    let superminority = if let Some(-2) = validator_meta.stake_concentration_score {
        1.0
    } else {
        0.0
    };

    // TODO make this an API call to fetch total SOL staked
    let total_stake_prop =
        validator_meta.active_stake.unwrap_or_default() as f64 / (TOTAL_SOLANA_STAKED_LAMPORTS);

    // Consensus mods score is -2 if modifications are detected
    let consensus_mods = if let Some(-2) = validator_meta.consensus_mods_score {
        1.0
    } else {
        0.0
    };
    (1.0 + DATA_CENTER_CONCENTRATION_WEIGHT * dc_concentration_score
        + PUBLISHED_INFORMATION_WEIGHT * published_information)
        / (1.0
            + SUPERMINORITY_WEIGHT * superminority
            + TOTAL_STAKE_PROP_WEIGHT * total_stake_prop
            + CONSENSUS_MODS_WEIGHT * consensus_mods)
}

mod tests {

    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use crate::fetcher::RecordFields;
    // Tests are checking the lower bound, default case, and upper bound of the scores, and
    // ensuring that the float math is acceptably precise within a threshold for our distribution of rewards
    /*
        Performance Score

        Expected Parameter Ranges:
        Skipped slot percent: (0, 1.0)
        Not delinquent: {0, 1}
        Running jito: {0, 1}
        Vote distance score: {0, 1, 2}
        Root distance score: {0, 1, 2}
    */
    #[test]
    fn test_performance_score_default() {
        let score = performance_score(
            &ValidatorsAppResponseEntry {
                ..Default::default()
            },
            0.0,
            false,
        );
        assert_eq!(score, 0.);
    }

    #[test]
    fn test_performance_score_maximum() {
        // Maximum in theory
        let score = performance_score(
            &ValidatorsAppResponseEntry {
                skipped_slot_percent: Some(String::from("0")),
                delinquent: Some(false),
                vote_distance_score: Some(2),
                root_distance_score: Some(2),
                software_version_score: Some(2),
                ..Default::default()
            },
            1.0,
            false,
        );
        assert!(f64::abs(score - 8.0) < 0.000000001);
    }

    #[test]
    fn test_performance_score_minimum() {
        // Minimum in theory
        let score = performance_score(
            &ValidatorsAppResponseEntry {
                skipped_slot_percent: Some(String::from("1")),
                delinquent: Some(false),
                vote_distance_score: Some(0),
                root_distance_score: Some(0),
                software_version_score: Some(0),
                ..Default::default()
            },
            0.0,
            false,
        );
        assert!(f64::abs(score) < 0.000000001);
    }

    /*
        Decentralization Score

        Expected Parameter Ranges:
        Skipped slot percent: (0, 1.0)
        Not delinquent: {0, 1}
        Running jito: {0, 1}
        Vote distance score: {0, 1, 2}
        Root distance score: {0, 1, 2}
        MEV Commission BPS: [0, 10000]
    */

    #[test]
    fn test_decentralization_score_minimum() {
        // minimum in theory
        let score = decentralization_score(&ValidatorsAppResponseEntry {
            data_center_concentration_score: Some(-2),
            published_information_score: Some(0),
            commission: Some(100),
            stake_concentration_score: Some(-2),
            active_stake: Some(TOTAL_SOLANA_STAKED_LAMPORTS as u64),
            consensus_mods_score: Some(-2),
            ..Default::default()
        });
        // https://www.wolframalpha.com/input?i2d=true&i=Divide%5B1%2B20*0%2B10*0%2B5*0%2B5*0%2C1%2B%5C%2840%29Divide%5B400000000*1000000000%2C400000000*1000000000%5D*10000%5C%2841%29+%2B+50*1%2B100*1%5D
        assert!(f64::abs(score - 0.000_194_137_060_764) < 0.000_000_001);
    }

    #[test]
    fn test_decentralization_score_maximum() {
        let score = decentralization_score(&ValidatorsAppResponseEntry {
            data_center_concentration_score: Some(0),
            published_information_score: Some(1),
            commission: Some(0),
            stake_concentration_score: Some(0),
            active_stake: Some(0u64),
            consensus_mods_score: Some(0),
            ..Default::default()
        });

        // https://www.wolframalpha.com/input?i2d=true&i=Divide%5B1%2B20*1%2B10*1%2B5*1+%2B+5*1%2C1%2B%5C%2840%29Divide%5B0%2C400000000*1000000000%5D*10000%5C%2841%29+%2B+50*0%2B100*0%5D
        assert!(f64::abs(score - 31.0) < 0.000000001);
    }
}
