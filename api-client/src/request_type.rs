use serde::{Deserialize, Serialize};

/// Request parameters for epoch-based queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochRequest {
    /// Epoch number
    pub epoch: u64,
}

/// Query parameters for paginated requests
#[derive(Debug, Clone, Default)]
pub struct QueryParams {
    /// Limit number of results
    pub limit: Option<u32>,

    /// Offset for pagination
    pub offset: Option<u32>,

    /// Epoch filter
    pub epoch: Option<u64>,

    /// Sort order (asc/desc)
    pub sort_order: Option<String>,
}

impl QueryParams {
    /// Create new query params with limit
    pub fn with_limit(limit: u32) -> Self {
        Self {
            limit: Some(limit),
            ..Default::default()
        }
    }

    /// Create new query params with epoch
    pub fn with_epoch(epoch: u64) -> Self {
        Self {
            epoch: Some(epoch),
            ..Default::default()
        }
    }

    /// Set limit
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set offset
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Set epoch
    pub fn epoch(mut self, epoch: u64) -> Self {
        self.epoch = Some(epoch);
        self
    }

    /// Convert to query string
    pub fn to_query_string(&self) -> String {
        let mut params = Vec::new();

        if let Some(limit) = self.limit {
            params.push(format!("limit={}", limit));
        }
        if let Some(offset) = self.offset {
            params.push(format!("offset={}", offset));
        }
        if let Some(epoch) = self.epoch {
            params.push(format!("epoch={}", epoch));
        }
        if let Some(ref sort_order) = self.sort_order {
            params.push(format!("sort_order={}", sort_order));
        }

        if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        }
    }
}
