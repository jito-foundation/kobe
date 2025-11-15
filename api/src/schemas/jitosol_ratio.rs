use super::stake_pool_stats::{DateTimeRangeFilter, F64DataPoint};
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Clone, Debug)]
pub struct JitoSolRatioRequest {
    pub range_filter: DateTimeRangeFilter,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct JitoSolRatioResponse {
    pub ratios: Vec<F64DataPoint>,
}

impl std::fmt::Display for JitoSolRatioRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}",
            self.range_filter.start.to_rfc3339(),
            self.range_filter.end.to_rfc3339()
        )
    }
}
