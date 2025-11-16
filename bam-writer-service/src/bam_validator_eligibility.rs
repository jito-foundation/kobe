//! Validator eligibility validation for BAM delegation (JIP-28)

use std::collections::HashMap;

use kobe_core::client_type::ClientType;
use validator_history::ValidatorHistory;

/// Validates validator eligibility for BAM delegation according to JIP-28 criteria
pub struct BamValidatorEligibility {
    /// Start epoch for lookback window
    start_epoch: u16,

    /// End epoch for lookback window
    end_epoch: u16,

    /// Chain maximum vote credits per epoch
    chain_max_credits: HashMap<u16, u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IneligibilityReason {
    NotBamClient,
    NonZeroCommission {
        epoch: u16,
        commission: u8,
    },
    HighMevCommission {
        epoch: u16,
        mev_commission: u16,
    },
    InSuperminority {
        epoch: u16,
    },
    LowVoteCredits {
        epoch: u16,
        credits: u32,
        min_required: u32,
    },
    InsufficientHistory,
}

impl BamValidatorEligibility {
    /// Create a new eligibility checker
    pub fn new(current_epoch: u64, all_validator_histories: &[ValidatorHistory]) -> Self {
        let start_epoch = (current_epoch - 3) as u16;
        let end_epoch = (current_epoch - 1) as u16;

        // Calculate chain maximum vote credits for each epoch
        let chain_max_credits =
            Self::calculate_chain_max_credits(all_validator_histories, start_epoch, end_epoch);

        Self {
            start_epoch,
            end_epoch,
            chain_max_credits,
        }
    }

    /// Calculate the maximum vote credits earned by any validator in each epoch
    fn calculate_chain_max_credits(
        all_histories: &[ValidatorHistory],
        start_epoch: u16,
        end_epoch: u16,
    ) -> HashMap<u16, u32> {
        let mut max_credits = HashMap::new();

        for epoch in start_epoch..=end_epoch {
            let max = all_histories
                .iter()
                .filter_map(|vh| {
                    vh.history
                        .epoch_credits_range(epoch, epoch)
                        .into_iter()
                        .flatten()
                        .next()
                })
                .max()
                .unwrap_or(0);

            max_credits.insert(epoch, max);
        }

        max_credits
    }

    /// Check if a validator is eligible for BAM delegation
    ///
    /// Returns `Ok(())` if eligible, or `Err(IneligibilityReason)` with the first failure
    pub fn check_eligibility(
        &self,
        validator_history: &ValidatorHistory,
    ) -> Result<(), IneligibilityReason> {
        let client_types = validator_history
            .history
            .client_type_range(self.start_epoch, self.end_epoch);
        let commissions = validator_history
            .history
            .commission_range(self.start_epoch, self.end_epoch);
        let mev_commissions = validator_history
            .history
            .mev_commission_range(self.start_epoch, self.end_epoch);
        let superminority = validator_history
            .history
            .superminority_range(self.start_epoch, self.end_epoch);
        let epoch_credits = validator_history
            .history
            .epoch_credits_range(self.start_epoch, self.end_epoch);

        // Count how many epochs have data
        let epochs_with_data = commissions.iter().filter(|c| c.is_some()).count();

        // Must have history for all 3 epochs (continuous operation requirement)
        if epochs_with_data < 3 {
            return Err(IneligibilityReason::InsufficientHistory);
        }

        for (i, epoch) in (self.start_epoch..=self.end_epoch).enumerate() {
            // BAM clients
            if let Some(client_type) = client_types[i] {
                if !matches!(ClientType::from_u8(client_type), ClientType::Bam) {
                    return Err(IneligibilityReason::NotBamClient);
                }
            }

            // 0% inflation commission
            if let Some(commission) = commissions[i] {
                if commission != 0 {
                    return Err(IneligibilityReason::NonZeroCommission { epoch, commission });
                }
            }

            // ≤10% MEV commission
            if let Some(mev_commission) = mev_commissions[i] {
                if mev_commission > 10 {
                    return Err(IneligibilityReason::HighMevCommission {
                        epoch,
                        mev_commission,
                    });
                }
            }

            // Non-superminority
            if let Some(is_superminority) = superminority[i] {
                if is_superminority != 0 {
                    return Err(IneligibilityReason::InSuperminority { epoch });
                }
            }

            // Within 3% of chain maximum vote credits
            if let Some(credits) = epoch_credits[i] {
                if let Some(&max_credits) = self.chain_max_credits.get(&epoch) {
                    let min_required = (max_credits as f64 * 0.97) as u32;

                    if credits < min_required {
                        return Err(IneligibilityReason::LowVoteCredits {
                            epoch,
                            credits,
                            min_required,
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_pubkey::Pubkey;
    use validator_history::{CircBuf, ValidatorHistory, ValidatorHistoryEntry};

    // Helper to create a mock validator history entry
    fn create_entry(
        epoch: u16,
        client_type: u8,
        commission: u8,
        mev_commission: u16,
        is_superminority: u8,
        epoch_credits: u32,
    ) -> ValidatorHistoryEntry {
        ValidatorHistoryEntry {
            epoch,
            commission,
            mev_commission,
            is_superminority,
            epoch_credits,
            client_type,
            ..Default::default()
        }
    }

    // Helper to create a validator history with entries
    fn create_validator_history(entries: Vec<ValidatorHistoryEntry>) -> ValidatorHistory {
        let mut history = ValidatorHistory {
            struct_version: 1,
            vote_account: Pubkey::new_unique(),
            index: 0,
            bump: 0,
            _padding0: [0; 7],
            last_ip_timestamp: 0,
            last_version_timestamp: 0,
            validator_age: 0,
            validator_age_last_updated_epoch: 0,
            _padding1: [0; 226],
            history: CircBuf::default(),
        };

        for entry in entries {
            history.history.arr[entry.epoch as usize % history.history.arr.len()] = entry;
        }
        history
    }

    #[test]
    fn test_eligible_validator_passes() {
        let vh1 = create_validator_history(vec![
            create_entry(97, 6, 0, 10, 0, 10000),
            create_entry(98, 6, 0, 10, 0, 10000),
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        let checker = BamValidatorEligibility::new(100, &[vh1.clone()]);

        assert!(checker.check_eligibility(&vh1).is_ok());
    }

    #[test]
    fn test_not_bam_client_fails() {
        let vh = create_validator_history(vec![
            create_entry(97, 2, 0, 10, 0, 10000), // Firedancer
            create_entry(98, 6, 5, 10, 0, 10000),
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        let checker = BamValidatorEligibility::new(100, &[vh.clone()]);

        assert_eq!(
            checker.check_eligibility(&vh),
            Err(IneligibilityReason::NotBamClient)
        );
    }

    #[test]
    fn test_non_zero_commission_fails() {
        let vh = create_validator_history(vec![
            create_entry(97, 6, 0, 10, 0, 10000),
            create_entry(98, 6, 5, 10, 0, 10000), // 5% commission
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        let checker = BamValidatorEligibility::new(100, &[vh.clone()]);

        assert_eq!(
            checker.check_eligibility(&vh),
            Err(IneligibilityReason::NonZeroCommission {
                epoch: 98,
                commission: 5
            })
        );
    }

    #[test]
    fn test_high_mev_commission_fails() {
        let vh = create_validator_history(vec![
            create_entry(97, 6, 0, 10, 0, 10000),
            create_entry(98, 6, 0, 15, 0, 10000), // 15% MEV commission
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        let checker = BamValidatorEligibility::new(100, &[vh.clone()]);

        assert_eq!(
            checker.check_eligibility(&vh),
            Err(IneligibilityReason::HighMevCommission {
                epoch: 98,
                mev_commission: 15
            })
        );
    }

    #[test]
    fn test_superminority_fails() {
        let vh = create_validator_history(vec![
            create_entry(97, 6, 0, 10, 0, 10000),
            create_entry(98, 6, 0, 10, 1, 10000), // In superminority
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        let checker = BamValidatorEligibility::new(100, &[vh.clone()]);

        assert_eq!(
            checker.check_eligibility(&vh),
            Err(IneligibilityReason::InSuperminority { epoch: 98 })
        );
    }

    #[test]
    fn test_low_vote_credits_fails() {
        let vh_good = create_validator_history(vec![
            create_entry(97, 6, 0, 10, 0, 10000),
            create_entry(98, 6, 0, 10, 0, 10000),
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        let vh_bad = create_validator_history(vec![
            create_entry(97, 6, 0, 10, 0, 10000),
            create_entry(98, 6, 0, 10, 0, 9600), // 96% of max (below 97%)
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        let checker = BamValidatorEligibility::new(100, &[vh_good, vh_bad.clone()]);

        assert_eq!(
            checker.check_eligibility(&vh_bad),
            Err(IneligibilityReason::LowVoteCredits {
                epoch: 98,
                credits: 9600,
                min_required: 9700, // 97% of 10000
            })
        );
    }

    #[test]
    fn test_insufficient_history_fails() {
        // Only 2 epochs instead of required 3
        let vh = create_validator_history(vec![
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        let checker = BamValidatorEligibility::new(100, &[vh.clone()]);

        assert_eq!(
            checker.check_eligibility(&vh),
            Err(IneligibilityReason::InsufficientHistory)
        );
    }

    #[test]
    fn test_exactly_97_percent_passes() {
        let vh_max = create_validator_history(vec![
            create_entry(97, 6, 0, 10, 0, 10000),
            create_entry(98, 6, 0, 10, 0, 10000),
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        let vh_97 = create_validator_history(vec![
            create_entry(97, 6, 0, 10, 0, 9700), // Exactly 97%
            create_entry(98, 6, 0, 10, 0, 9700),
            create_entry(99, 6, 0, 10, 0, 9700),
            create_entry(100, 6, 0, 10, 0, 9700),
        ]);

        let checker = BamValidatorEligibility::new(100, &[vh_max, vh_97.clone()]);

        assert!(checker.check_eligibility(&vh_97).is_ok());
    }

    #[test]
    fn test_mev_commission_boundary() {
        // Exactly 10% MEV commission should pass
        let vh_10 = create_validator_history(vec![
            create_entry(97, 6, 0, 10, 0, 10000),
            create_entry(98, 6, 0, 10, 0, 10000),
            create_entry(99, 6, 0, 10, 0, 10000),
            create_entry(100, 6, 0, 10, 0, 10000),
        ]);

        // 11% should fail
        let vh_11 = create_validator_history(vec![
            create_entry(97, 6, 0, 11, 0, 10000),
            create_entry(98, 6, 0, 11, 0, 10000),
            create_entry(99, 6, 0, 11, 0, 10000),
            create_entry(100, 6, 0, 11, 0, 10000),
        ]);

        let checker = BamValidatorEligibility::new(100, &[vh_10.clone(), vh_11.clone()]);

        assert!(checker.check_eligibility(&vh_10).is_ok());
        assert!(checker.check_eligibility(&vh_11).is_err());
    }
}
