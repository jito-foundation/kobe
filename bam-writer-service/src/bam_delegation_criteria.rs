use jito_steward::constants::BASIS_POINTS_MAX;
use kobe_core::db_models::bam_epoch_metric::BamEpochMetric;

/// Criteria for BAM delegation amount based on JIP-28 specification
pub(crate) struct BamDelegationCriteria {
    /// JIP-28 tier breakpoints
    ///
    /// 0: stakeweight_threshold (in BPS)
    /// 1: allocation_pct (in BPS)
    tiers: Vec<(u64, u64)>,
}

impl BamDelegationCriteria {
    /// Create a new calculator with JIP-28 default tiers
    pub fn new() -> Self {
        Self {
            tiers: vec![
                (0, 2_000),      // 0% -> 20%
                (2_000, 3_000),  // 20% -> 30%
                (2_500, 4_000),  // 25% -> 40%
                (3_000, 5_000),  // 30% -> 50%
                (3_500, 7_000),  // 35% -> 70%
                (4_000, 10_000), // 40% -> 100%
            ],
        }
    }

    /// Calculate BAM stakeweight (percentage of network stake running BAM)
    fn calculate_bam_stakeweight(&self, bam_sol_stake: u64, total_sol_stake: u64) -> Option<u64> {
        if total_sol_stake == 0 {
            return None;
        }

        let stakeweight_bps = (bam_sol_stake as u128)
            .checked_mul(BASIS_POINTS_MAX as u128)?
            .checked_div(total_sol_stake as u128)?;

        Some(stakeweight_bps as u64)
    }

    /// Calculate current tier level with two-epoch validation
    pub fn calculate_current_allocation(
        &self,
        current_epoch_metric: &BamEpochMetric,
        previous_epoch_metric: Option<&BamEpochMetric>,
    ) -> u64 {
        let current_stakeweight_bps = self
            .calculate_bam_stakeweight(
                current_epoch_metric.get_bam_stake(),
                current_epoch_metric.get_total_stake(),
            )
            .unwrap_or(0);

        // If no previous epoch, return initial 20% (2000 BPS)
        let Some(prev_metric) = previous_epoch_metric else {
            return 2_000;
        };

        let previous_stakeweight_bps = self
            .calculate_bam_stakeweight(prev_metric.get_bam_stake(), prev_metric.get_total_stake())
            .unwrap_or(0);

        // Find highest tier where BOTH epochs meet threshold
        self.tiers
            .iter()
            .rev()
            .find(|(threshold, _)| {
                current_stakeweight_bps >= *threshold && previous_stakeweight_bps >= *threshold
            })
            .map(|(_, allocation)| *allocation)
            .unwrap_or(2_000)
    }

    /// Calculate available delegation amount in lamports
    pub fn calculate_available_delegation(
        &self,
        allocation_bps: u64,
        total_jitosol_tvl: u64,
    ) -> u64 {
        (total_jitosol_tvl as u128)
            .saturating_mul(allocation_bps as u128)
            .saturating_div(BASIS_POINTS_MAX as u128) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stakeweight_calculation() {
        let criteria = BamDelegationCriteria::new();

        // 25% = 2500 BPS
        assert_eq!(
            criteria.calculate_bam_stakeweight(100_000_000, 400_000_000),
            Some(2_500)
        );

        // 0% = 0 BPS
        assert_eq!(criteria.calculate_bam_stakeweight(0, 400_000_000), Some(0));

        // Division by zero
        assert_eq!(criteria.calculate_bam_stakeweight(100_000_000, 0), None);
    }

    #[test]
    fn test_two_epoch_validation_initial_epoch() {
        let criteria = BamDelegationCriteria::new();

        // First epoch ever - no previous data
        let current = BamEpochMetric::new(
            100,
            100_000_000, // 25% stakeweight
            400_000_000,
            10,
        );

        // Should only get initial 20% regardless of current stakeweight
        assert_eq!(criteria.calculate_current_allocation(&current, None), 2000);
    }

    #[test]
    fn test_two_epoch_validation_tier_advancement() {
        let criteria = BamDelegationCriteria::new();

        let previous = BamEpochMetric::new(
            99,
            100_000_000, // 25% stakeweight
            400_000_000,
            10,
        );

        let current = BamEpochMetric::new(
            100,
            100_000_000, // 25% stakeweight (same as previous)
            400_000_000,
            10,
        );

        // Both epochs at 25% -> should advance to 40% allocation
        assert_eq!(
            criteria.calculate_current_allocation(&current, Some(&previous)),
            4000
        );
    }

    #[test]
    fn test_two_epoch_validation_insufficient_previous_epoch() {
        let criteria = BamDelegationCriteria::new();

        let previous = BamEpochMetric::new(
            99,
            80_000_000, // 20% stakeweight (below 25% threshold)
            400_000_000,
            10,
        );

        let current = BamEpochMetric::new(
            100,
            100_000_000, // 25% stakeweight
            400_000_000,
            10,
        );

        // Current is 25% but previous was only 20%
        // Should stay at 30% tier (20% threshold met in both)
        assert_eq!(
            criteria.calculate_current_allocation(&current, Some(&previous)),
            3000
        );
    }

    #[test]
    fn test_two_epoch_validation_insufficient_current_epoch() {
        let criteria = BamDelegationCriteria::new();

        let previous = BamEpochMetric::new(
            99,
            100_000_000, // 25% stakeweight
            400_000_000,
            10,
        );

        let current = BamEpochMetric::new(
            100,
            80_000_000, // 20% stakeweight (dropped below threshold)
            400_000_000,
            10,
        );

        // Previous was 25% but current dropped to 20%
        // Should fall back to 30% tier (20% threshold met in both)
        assert_eq!(
            criteria.calculate_current_allocation(&current, Some(&previous)),
            3000
        );
    }

    #[test]
    fn test_two_epoch_validation_volatility_protection() {
        let criteria = BamDelegationCriteria::new();

        // Epoch N-1: 24% (just below 25% threshold)
        let previous = BamEpochMetric::new(99, 96_000_000, 400_000_000, 10);

        // Epoch N: 26% (just above 25% threshold)
        let current = BamEpochMetric::new(100, 104_000_000, 400_000_000, 10);

        // Even though current is above 25%, previous wasn't
        // Should stay at 30% (20% tier) not jump to 40% (25% tier)
        assert_eq!(
            criteria.calculate_current_allocation(&current, Some(&previous)),
            3000
        );
    }

    #[test]
    fn test_two_epoch_validation_highest_tier() {
        let criteria = BamDelegationCriteria::new();

        let previous = BamEpochMetric::new(
            99,
            160_000_000, // 40% stakeweight
            400_000_000,
            10,
        );

        let current = BamEpochMetric::new(
            100,
            180_000_000, // 45% stakeweight
            400_000_000,
            10,
        );

        // Both epochs above 40% threshold -> 100% allocation
        assert_eq!(
            criteria.calculate_current_allocation(&current, Some(&previous)),
            10_000
        );
    }

    #[test]
    fn test_two_epoch_validation_multiple_tier_jump_prevented() {
        let criteria = BamDelegationCriteria::new();

        // Previous: 15% stakeweight
        let previous = BamEpochMetric::new(99, 60_000_000, 400_000_000, 10);

        // Current: 35% stakeweight (jumped 20 percentage points!)
        let current = BamEpochMetric::new(100, 140_000_000, 400_000_000, 10);

        // Can't skip tiers - previous only qualified for initial 20%
        // Should get 20% allocation, not 70%
        assert_eq!(
            criteria.calculate_current_allocation(&current, Some(&previous)),
            2000
        );
    }

    #[test]
    fn test_two_epoch_validation_gradual_progression() {
        let criteria = BamDelegationCriteria::new();

        // Simulate progression through tiers

        // Epoch 1: Both at 20% -> 30% allocation
        let epoch_1 = BamEpochMetric::new(100, 80_000_000, 400_000_000, 10);
        let epoch_2 = BamEpochMetric::new(101, 80_000_000, 400_000_000, 10);
        assert_eq!(
            criteria.calculate_current_allocation(&epoch_2, Some(&epoch_1)),
            3000
        );

        // Epoch 3: Both at 25% -> 40% allocation
        let epoch_3 = BamEpochMetric::new(102, 100_000_000, 400_000_000, 10);
        assert_eq!(
            criteria.calculate_current_allocation(&epoch_3, Some(&epoch_2)),
            3000
        );

        let epoch_4 = BamEpochMetric::new(103, 100_000_000, 400_000_000, 10);
        assert_eq!(
            criteria.calculate_current_allocation(&epoch_4, Some(&epoch_3)),
            4000
        );
    }

    #[test]
    fn test_two_epoch_validation_edge_case_zero_stake() {
        let criteria = BamDelegationCriteria::new();

        let previous = BamEpochMetric::new(
            99,
            0, // No BAM stake
            400_000_000,
            0,
        );

        let current = BamEpochMetric::new(
            100,
            100_000_000, // Suddenly 25% stake
            400_000_000,
            10,
        );

        // Previous had 0%, so can only get initial 20%
        assert_eq!(
            criteria.calculate_current_allocation(&current, Some(&previous)),
            2000
        );
    }
}
