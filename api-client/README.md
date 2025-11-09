# Kobe API Client

A comprehensive, async Rust client library for interacting with [Jito Network](https://jito.network/) APIs.

[![Crates.io](https://img.shields.io/crates/v/kobe-api-client.svg)](https://crates.io/crates/kobe-client)
[![Documentation](https://docs.rs/kobe-api-client/badge.svg)](https://docs.rs/kobe-client)
[![License](https://img.shields.io/crates/l/kobe-api-client.svg)](LICENSE)

## Features

- **MEV & Staker Rewards API**: Query MEV and priority fee rewards for stakers and validators
- **Stake Pool API**: Access validator statistics, JitoSOL metrics, and network data
- **StakeNet API**: On-chain validator history and performance data
- **Async/Await**: Built on `tokio` and `reqwest` for high-performance async operations
- **Type-Safe**: Strongly typed request and response structures
- **Error Handling**: Comprehensive error types with detailed messages
- **Retry Logic**: Automatic retry with exponential backoff
- **Configurable**: Flexible configuration options for timeouts, retries, and more

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
kobe-api-client = "0.1"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use kobe_api_client::client::KobeApiClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a client with mainnet defaults
    let client = KobeApiClient::mainnet();

    // Get staker rewards
    let rewards = client.get_staker_rewards(Some(10)).await?;
    println!("Found {} staker rewards", rewards.rewards.len());

    // Get validator information
    let validators = client.get_validators(None).await?;
    println!("Found {} validators", validators.validators.len());

    // Get MEV rewards for the network
    let mev_rewards = client.get_mev_rewards(None).await?;
    println!("Epoch: {}, Total MEV: {} lamports",
             mev_rewards.epoch,
             mev_rewards.total_network_mev_lamports);

    Ok(())
}
```

## Usage Examples

### MEV & Staker Rewards API

#### Get Staker Rewards

```rust
use kobe_client::client::KobeApiClient;

let client = KobeApiClient::mainnet();

// Get top 5 staker rewards
let rewards = client.get_staker_rewards(Some(5)).await?;

for reward in rewards.rewards {
    println!("Stake Account: {}", reward.stake_account);
    println!("MEV Rewards: {} lamports", reward.mev_rewards);
    println!("Claimed: {}", reward.mev_claimed);
}
```

#### Get Validator Rewards

```rust
// Get validator rewards for a specific epoch
let validator_rewards = client.get_validator_rewards(Some(678), Some(10)).await?;

for validator in validator_rewards.validators {
    println!("Vote Account: {}", validator.vote_account);
    println!("MEV Rewards: {} lamports", validator.mev_rewards);
    println!("MEV Commission: {} bps", validator.mev_commission_bps);
}
```

### Stake Pool API

#### Get Validator Information

```rust
// Get all validators for the current epoch
let validators = client.get_validators(None).await?;

// Get validators for a specific epoch
let validators_600 = client.get_validators(Some(600)).await?;

// Filter validators running Jito
let jito_validators: Vec<_> = validators.validators
    .into_iter()
    .filter(|v| v.running_jito)
    .collect();

println!("Found {} Jito validators", jito_validators.len());
```

#### Get Validator History

```rust
// Get historical data for a specific validator
let vote_account = "GdRKUZKdiXMEATjddQW6q4W8bPgXRBYJKayfeqdQcEPa";
let history = client.get_validator_history(vote_account).await?;

for entry in history.iter().take(5) {
    println!("Epoch {}: {} lamports", entry.epoch, entry.mev_rewards);
}
```

#### Get MEV Network Statistics

```rust
// Get current epoch MEV stats
let mev_stats = client.get_mev_rewards(None).await?;
println!("Total Network MEV: {} lamports", mev_stats.total_network_mev_lamports);
println!("MEV per lamport: {}", mev_stats.mev_reward_per_lamport);

// Get MEV stats for a specific epoch
let mev_stats_600 = client.get_mev_rewards(Some(600)).await?;
```

#### Get JitoSOL Metrics

```rust
use chrono::{Duration, Utc};

// Get JitoSOL to SOL ratio for the last 7 days
let end = Utc::now();
let start = end - Duration::days(7);
let ratio = client.get_jitosol_sol_ratio(start, end).await?;

for point in ratio.ratios {
    println!("{}: {}", point.date, point.data);
}
```

#### Get MEV Commission Averages

```rust
// Get historical MEV commission averages with APY and TVL data
let commission_data = client.get_mev_commission_average_over_time().await?;

println!("Aggregated MEV Rewards: {}", commission_data.aggregated_mev_rewards);

// Print APY data
for apy_point in commission_data.apy {
    println!("{}: {:.2}%", apy_point.date, apy_point.data * 100.0);
}
```

### Convenience Methods

```rust
// Get current epoch
let current_epoch = client.get_current_epoch().await?;
println!("Current epoch: {}", current_epoch);

// Get only Jito-running validators
let jito_validators = client.get_jito_validators().await?;

// Get top 10 validators by MEV rewards
let top_validators = client.get_validators_by_mev_rewards(None, 10).await?;

// Check if a validator is running Jito
let is_jito = client.is_validator_running_jito(
    "GdRKUZKdiXMEATjddQW6q4W8bPgXRBYJKayfeqdQcEPa"
).await?;

// Get validator MEV commission
let commission = client.get_validator_mev_commission(
    "GdRKUZKdiXMEATjddQW6q4W8bPgXRBYJKayfeqdQcEPa"
).await?;

// Calculate total MEV rewards across multiple epochs
let total_mev = client.calculate_total_mev_rewards(600, 610).await?;
println!("Total MEV from epoch 600-610: {} lamports", total_mev);
```

## Configuration

### Using the Builder Pattern

```rust
use std::time::Duration;

use kobe_client::client::KobeApiClientBuilder;

let client = KobeApiClientBuilder::new()
    .timeout(Duration::from_secs(60))
    .user_agent("my-app/1.0")
    .retry(true)
    .max_retries(5)
    .build();
```

### Using Config

```rust
use kobe_client::{KobeApiClient, Config};
use std::time::Duration;

let config = Config::mainnet()
    .with_timeout(Duration::from_secs(60))
    .with_user_agent("my-app/1.0")
    .with_retry(true)
    .with_max_retries(5);

let client = JitoClient::new(config);
```

### Custom Base URL

```rust
let config = Config::custom("https://custom-api.example.com");
let client = JitoClient::new(config);
```

## Error Handling

The library provides detailed error types:

```rust
use kobe_client::{KobeApiClient, JitoError};

let client = KobeApiClient::mainnet();

match client.get_staker_rewards(Some(10)).await {
    Ok(rewards) => println!("Success: {} rewards", rewards.rewards.len()),
    Err(JitoError::RateLimitExceeded) => {
        eprintln!("Rate limit exceeded, please wait");
    }
    Err(JitoError::NotFound(msg)) => {
        eprintln!("Resource not found: {}", msg);
    }
    Err(JitoError::ApiError { status_code, message }) => {
        eprintln!("API error {}: {}", status_code, message);
    }
    Err(e) => eprintln!("Other error: {}", e),
}
```

## Advanced Usage

### Query Parameters

```rust
use kobe_client::QueryParams;

let params = QueryParams::default()
    .limit(50)
    .offset(100)
    .epoch(600);

let rewards = client.get_staker_rewards_with_params(&params).await?;
```

### Retry Logic

The client automatically retries failed requests with exponential backoff. You can configure this behavior:

```rust
let client = KobeApiClientBuilder::new()
    .retry(true)           // Enable retries
    .max_retries(3)        // Maximum 3 retry attempts
    .build();
```

Retries are attempted for:
- Network timeouts
- Connection errors
- Temporary network issues

Retries are NOT attempted for:
- Invalid parameters (400)
- Not found errors (404)
- Rate limiting (429)
- Server errors (5xx)

## API Documentation

For detailed API documentation, visit:
- [MEV & Staker Rewards API](https://www.jito.network/docs/jitosol/jitosol-liquid-staking/for-developers/mev-and-staker-rewards-api-info/)
- [Stake Pool API](https://www.jito.network/docs/jitosol/jitosol-liquid-staking/for-developers/stake-pool-api/)
- [StakeNet API](https://www.jito.network/docs/jitosol/jitosol-liquid-staking/for-developers/stakenet-api/)

## Examples

Check the `examples/` directory for complete working examples:

```bash
# Run the basic example
cargo run --example validators

# Run the validator analysis example
cargo run --example validator_analysis

# Run the MEV tracking example
cargo run --example mev_tracking
```

## Testing

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_config_builder
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under either of:

- Apache License, Version 2.0 ([LICENSE](LICENSE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

## Disclaimer

This is an unofficial client library and is not affiliated with or endorsed by Jito Labs or the Jito Foundation.

## Links

- [Jito Network](https://jito.network/)
- [Jito Documentation](https://www.jito.network/docs/)
- [Jito GitHub](https://github.com/jito-foundation)
