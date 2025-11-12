pub mod client;
pub mod client_builder;
pub mod config;
pub mod error;
pub mod request_type;
pub mod response_type;
pub mod staker_reward;

/// Default base URL for mainnet Kobe API
pub const MAINNET_BASE_URL: &str = "https://kobe.mainnet.jito.network";

/// Default base URL for testnet Kobe API
pub const TESTNET_BASE_URL: &str = "https://kobe.testnet.jito.network";

/// API version
pub const API_VERSION: &str = "v1";
