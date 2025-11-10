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
    /// * `bam_sol_stake` - Total stake of all BAM validators
    /// * `total_sol_stake` - Total stake across entire Solana network
    /// * `total_jitosol_tvl` - Total value locked in JitoSOL stake pool
    ///
    /// # Returns
    /// Total JitoSOL amount available for delegation to all BAM validators
    pub fn calculate_available_delegation(
        &self,
        bam_sol_stake: u64,
        total_sol_stake: u64,
        total_jitosol_tvl: u64,
    ) -> u64 {
        let stakeweight = match self.calculate_bam_stakeweight(bam_sol_stake, total_sol_stake) {
            Some(sw) => sw,
            None => return 0,
        };

        let allocation_pct = self.get_allocation_percentage(stakeweight);
        (total_jitosol_tvl as f64 * allocation_pct) as u64
    }

    // Calculate pro-rata delegation for a specific BAM validator
    // pub fn calculate_validator_delegation(
    //     &self,
    //     validator_stake: u64,
    //     total_bam_stake: u64,
    //     total_available_delegation: u64,
    // ) -> u64 {
    //     if total_bam_stake == 0 {
    //         return 0;
    //     }

    //     let validator_share = validator_stake as f64 / total_bam_stake as f64;
    //     (total_available_delegation as f64 * validator_share) as u64
    // }
}
