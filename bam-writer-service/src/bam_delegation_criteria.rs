/// Criteria for BAM delegation amount based on JIP-28 specification
pub(crate) struct BamDelegationCriteria {
    /// JIP-28 tier breakpoints
    ///
    /// 0: stakeweight_threshold
    /// 1: allocation_pct
    tiers: Vec<(f64, f64)>,
}

impl BamDelegationCriteria {
    /// Create a new calculator with JIP-28 default tiers
    pub fn new() -> Self {
        Self {
            tiers: vec![
                (0.00, 0.20), // Initial -> 20%
                (0.20, 0.30), // 20% -> 30%
                (0.25, 0.40), // 25% -> 40%
                (0.30, 0.50), // 30% -> 50%
                (0.35, 0.70), // 35% -> 70%
                (0.40, 1.00), // 40% -> 100%
            ],
        }
    }

    /// Calculate BAM stakeweight (percentage of network stake running BAM)
    fn calculate_bam_stakeweight(&self, bam_sol_stake: u64, total_sol_stake: u64) -> Option<f64> {
        if total_sol_stake == 0 {
            return None;
        }
        Some(bam_sol_stake as f64 / total_sol_stake as f64)
    }

    /// Get JitoSOL allocation percentage for a given BAM stakeweight
    fn get_allocation_percentage(&self, bam_stakeweight: f64) -> f64 {
        self.tiers
            .iter()
            .rev()
            .find(|(threshold, _)| bam_stakeweight >= *threshold)
            .map(|(_, allocation)| *allocation)
            .unwrap_or(0.20) // Default to initial 20%
    }

    /// Calculate total available BAM delegation stake
    ///
    /// # Arguments
    /// * `bam_stake` - Total stake of all BAM validators
    /// * `total_stake` - Total stake across entire Solana network
    /// * `total_jitosol_tvl` - Total value locked in JitoSOL stake pool
    ///
    /// # Returns
    /// Total JitoSOL amount available for delegation to all BAM validators
    pub fn calculate_available_delegation(
        &self,
        bam_stake: u64,
        total_stake: u64,
        total_jitosol_tvl: u64,
    ) -> u64 {
        let stakeweight = match self.calculate_bam_stakeweight(bam_stake, total_stake) {
            Some(sw) => sw,
            None => return 0,
        };

        let allocation_pct = self.get_allocation_percentage(stakeweight);
        (total_jitosol_tvl as f64 * allocation_pct) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stakeweight_calculation() {
        let criteria = BamDelegationCriteria::new();

        assert_eq!(
            criteria.calculate_bam_stakeweight(100_000_000, 400_000_000),
            Some(0.25)
        );
        assert_eq!(
            criteria.calculate_bam_stakeweight(0, 400_000_000),
            Some(0.0)
        );
        assert_eq!(criteria.calculate_bam_stakeweight(100_000_000, 0), None);
    }

    #[test]
    fn test_allocation_tiers() {
        let criteria = BamDelegationCriteria::new();

        // Test each tier boundary
        assert_eq!(criteria.get_allocation_percentage(0.00), 0.20);
        assert_eq!(criteria.get_allocation_percentage(0.19), 0.20);
        assert_eq!(criteria.get_allocation_percentage(0.20), 0.30);
        assert_eq!(criteria.get_allocation_percentage(0.24), 0.30);
        assert_eq!(criteria.get_allocation_percentage(0.25), 0.40);
        assert_eq!(criteria.get_allocation_percentage(0.30), 0.50);
        assert_eq!(criteria.get_allocation_percentage(0.35), 0.70);
        assert_eq!(criteria.get_allocation_percentage(0.40), 1.00);
        assert_eq!(criteria.get_allocation_percentage(0.50), 1.00);
    }

    #[test]
    fn test_available_delegation() {
        let criteria = BamDelegationCriteria::new();

        // 25% BAM stakeweight -> 40% JitoSOL allocation
        let result = criteria.calculate_available_delegation(
            100_000_000, // 100M SOL in BAM
            400_000_000, // 400M SOL total (25%)
            10_000_000,  // 10M JitoSOL TVL
        );
        assert_eq!(result, 4_000_000); // 40% of 10M = 4M

        // Edge case: 0 total stake
        let result = criteria.calculate_available_delegation(100_000_000, 0, 10_000_000);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_jip28_spec_example() {
        let criteria = BamDelegationCriteria::new();

        // Scenario from JIP-28:
        // If BAM has 35% of network stake, they get 70% of JitoSOL TVL
        let result = criteria.calculate_available_delegation(
            140_000_000, // 140M SOL in BAM (35%)
            400_000_000, // 400M total network stake
            10_000_000,  // 10M JitoSOL TVL
        );
        assert_eq!(result, 7_000_000); // 70% of 10M = 7M
    }
}
