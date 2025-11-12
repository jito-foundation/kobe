use chrono::{DateTime, Duration, DurationRound, Utc};
use kobe_core::SortOrder;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

#[derive(Clone, Copy, Eq, PartialEq, Default, Serialize, Deserialize, Debug)]
pub enum BucketType {
    #[default]
    Daily,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateTimeRangeFilter {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl DateTimeRangeFilter {
    fn is_valid(&self) -> bool {
        if self.start.duration_round(Duration::hours(1)).is_err()
            || self.end.duration_round(Duration::hours(1)).is_err()
        {
            return false;
        }
        self.start < self.end
    }
}

impl Default for DateTimeRangeFilter {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            start: now - Duration::weeks(1),
            end: now,
        }
    }
}

// Rounds to nearest hour for cache optimization, returning the original if rounding fails.
pub fn round_to_hour(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt.duration_round(Duration::hours(1)).unwrap_or(dt)
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub enum SortField {
    BlockTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortBy {
    pub field: SortField,
    pub order: SortOrder,
}

impl Default for SortBy {
    fn default() -> Self {
        Self {
            field: SortField::BlockTime,
            order: SortOrder::Asc,
        }
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct GetStakePoolStatsRequest {
    /// Specifies how to bucket the data.
    pub bucket_type: BucketType,

    /// Fetches data within this range.
    pub range_filter: DateTimeRangeFilter,

    /// Declares how to sort the returned data.
    pub sort_by: SortBy,
}

impl GetStakePoolStatsRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if !self.range_filter.is_valid() {
            return Err(ApiError::validation_error(
                "Invalid data range: start must be before end",
            ));
        }

        Ok(())
    }
}

impl std::fmt::Display for GetStakePoolStatsRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}-{:?}-{:?}-{:?}-{:?}",
            self.bucket_type,
            round_to_hour(self.range_filter.start).to_string(),
            round_to_hour(self.range_filter.end).to_string(),
            self.sort_by.field,
            self.sort_by.order
        )
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct GetStakePoolStatsResponse {
    /// A summation of the mev_rewards data points.
    pub aggregated_mev_rewards: i64,

    /// MEV rewards over time.
    pub mev_rewards: Vec<I64DataPoint>,

    /// Stake pool TVL over time.
    pub tvl: Vec<I64DataPoint>,

    /// Stake pool apy over time.
    pub apy: Vec<F64DataPoint>,

    /// Total validators in the pool's validator set over time.
    pub num_validators: Vec<I64DataPoint>,

    /// Total JitoSOL supply over time.
    pub supply: Vec<F64DataPoint>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct I64DataPoint {
    pub data: i64,
    pub date: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct F64DataPoint {
    pub data: f64,
    pub date: DateTime<Utc>,
}
