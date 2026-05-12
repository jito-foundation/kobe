use kobe_core::db_models::coinbase_balances::CoinbaseBalance;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct CbBalanceRequest {
    pub epoch: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoinbaseBalanceResponse {
    pub coinbase_balance: Option<CoinbaseBalance>,
}
