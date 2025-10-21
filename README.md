# Kobe 🥩

Kobe is the internal name for the Jito Foundation's liquid stake pool infrastructure - a comprehensive suite of services powering JitoSOL and providing MEV rewards distribution on Solana.

## What is Kobe?

Kobe represents the complete backend infrastructure for Jito's liquid staking ecosystem.
Named after both the premium grade of Kobe beef and in honor of basketball legend Kobe Bryant.

## Architecture Overview

```
                       ┌───────────────────────────────────────────────────────┐
                       │                    Solana Network                     │
                       │                    (Blockchain)                       │
                       └───────────────────────────────────────────────────────┘
                         ▲             │                         │
                         │             │  (read on-chain data)   │
                         │             ▼                         ▼
             (write      │      ┌──────────────────┐    ┌──────────────────┐
          transactions)  │      │  Kobe Writer     │    │ Steward Writer   │
                         │      │   Service        │    │    Service       │
                         │      │ (Data Collection)│    │ (Steward Events) │
                         │      └──────────────────┘    └──────────────────┘
                         │             │                        │
                         │             ▼ (write to db)          ▼ (write to db)
          ┌──────────────────┐   ┌─────────────────────────────────────────────┐
          │  Kobe Cranker    │   │              MongoDB                        │
          │ (Pool Management)│   │            (Database)                       │
          └──────────────────┘   └─────────────────────────────────────────────┘
                                                   │
                                                   │ (read from db + on-chain)
                                                   ▼
                                         ┌─────────────────┐
                                         │   Kobe API      │
                                         │  (Data Access)  │
                                         └─────────────────┘
```

## JitoSOL APY Calculation

JitoSOL's Annual Percentage Yield (APY) is calculated using epoch-over-epoch growth rates of the stake pool, incorporating both staking rewards and MEV distributions.

### Single Epoch APY Calculation

#### Quick APY Calculation

```rust
/// Simple APY calculation based on previous epoch and current epoch values
/// NOTE: This assumes the current epoch length will remain constant for the entire year
pub fn get_stake_pool_apy(stake_pool: &StakePool, slot_ms: u64) -> f64 {
    let seconds_per_epoch = DEFAULT_SLOTS_PER_EPOCH * slot_ms / 1000;
    let epochs_per_year = 365.25 * 3600.0 * 24.0 / seconds_per_epoch as f64;
    let epoch_rate = (stake_pool.total_lamports as f64 / stake_pool.pool_token_supply as f64)
        / (stake_pool.last_epoch_total_lamports as f64
            / stake_pool.last_epoch_pool_token_supply as f64);
    epoch_rate.powf(epochs_per_year) - 1.0
}
```

#### Key Components

- **Epoch Growth Rate**: Compares current vs previous epoch stake pool ratios
- **Annualization**: Compounds the epoch rate over ~162 epochs per year (based on current slot timing)
- **MEV Integration**: Includes MEV rewards distributed to the stake pool
- **⚠️ Important Limitation**: This calculation assumes current epoch length remains constant for the entire year, which may not reflect actual network conditions

### API APY Calculation (Moving Average)

When retrieving APY through the `stake_pool_stats` endpoint, additional smoothing is applied:

#### Moving Average Processing

```rust
// Applied after aggregating daily data
let new_stake_pool_stats = Self::calculate_moving_avg_apy(&docs, 10).unwrap();
```

#### ⚠️Important: Date Range Requirements

The moving average calculation requires MORE than 10 epochs in the date range to execute:

- < 10 epochs in range: No moving average applied - returns all raw APY values
- = 10 epochs in range: Still no moving average applied
- > 10 epochs in range: Moving averages calculated only for epochs with sufficient history

This means short date range queries (e.g., 2-3 days) will return raw APY values, while longer queries will return smoothed values.

#### Example

##### Sample Data (Raw APY per Epoch)
```
Epoch | Raw APY | Moving Avg APY (10-epoch window)
------|---------|----------------------------------
580   | 7.2%    | N/A (not enough history)
581   | 8.1%    | N/A (not enough history)
582   | 6.8%    | N/A (not enough history)
583   | 9.2%    | N/A (not enough history)
584   | 5.9%    | N/A (not enough history)
585   | 7.8%    | N/A (not enough history)
586   | 8.4%    | N/A (not enough history)
587   | 6.3%    | N/A (not enough history)
588   | 7.9%    | N/A (not enough history)
589   | 8.7%    | 7.49% ← First moving average (epochs 580-589)
590   | 7.1%    | 7.47% ← (epochs 581-590)
591   | 8.9%    | 7.66% ← (epochs 582-591)
592   | 6.4%    | 7.51% ← (epochs 583-592)
593   | 7.6%    | 7.58% ← (epochs 584-593)
594   | 8.2%    | 7.66% ← (epochs 585-594)
595   | 7.3%    | 7.64% ← (epochs 586-595)
596   | 8.8%    | 7.77% ← (epochs 587-596)
597   | 6.7%    | 7.66% ← (epochs 588-597)
598   | 7.4%    | 7.60% ← (epochs 589-598)
599   | 8.1%    | 7.65% ← (epochs 590-599)
```

##### Detailed Calculation for Epoch 599

###### Step 1: Identify Window
- **Target Epoch**: 599
- **Window Size**: 10 epochs
- **Epochs Used**: 590, 591, 592, 593, 594, 595, 596, 597, 598, 599

###### Step 2: Collect Raw APY Values
```
590: 7.1%
591: 8.9%
592: 6.4%
593: 7.6%
594: 8.2%
595: 7.3%
596: 8.8%
597: 6.7%
598: 7.4%
599: 8.1%
```

###### Step 3: Calculate Average

```
Sum = 7.1 + 8.9 + 6.4 + 7.6 + 8.2 + 7.3 + 8.8 + 6.7 + 7.4 + 8.1 = 76.5%
Moving Average = 76.5% ÷ 10 = 7.65%
```

###### Result

- **Raw APY for Epoch 599**: 8.1%
- **API Returns**: 7.65% (moving average)

##### Visual Comparison

###### Raw APY Pattern

```
   9% |     *           *
   8% |   *   *   *   *     *   *
   7% | *       *   *   * *   *
   6% |         *           *
   5% |     *
      +-------------------------
       580 582 584 586 588 590 592 594 596 598
```

###### Moving Average APY Pattern

```
   9% |
   8% |
   7% |     ~~~~~~~~~~~~~~~~~~~
   6% |
   5% |
      +-------------------------
       580 582 584 586 588 590 592 594 596 598
```

##### Why This Matters

| Aspect | Raw APY | Moving Average APY |
|--------|---------|-------------------|
| **Volatility** | High (5.9% to 9.2%) | Low (7.47% to 7.77%) |
| **User Experience** | Confusing jumps | Stable trends |
| **Responsiveness** | Immediate | Gradual |
| **Use Case** | Internal monitoring | Public API display |

##### Code Flow Summary

1. **Database Storage**: Each epoch stores its raw APY (e.g., 8.1% for epoch 599)
2. **API Aggregation**: Groups data into daily buckets
3. **Moving Average**: Calculates 10-epoch rolling average
4. **API Response**: Returns smoothed values (7.65% instead of 8.1%)

Therefore, users may see different APY values between real-time calculations and API responses.

## Crates

### [Kobe API](./api/README.md)
**RESTful API service** providing access to MEV rewards, validator performance metrics, and stake pool analytics.

**Key Endpoints:**
- MEV & priority fee reward queries
- Validator performance and rankings
- JitoSOL stake pool metrics
- Historical trend analysis

**Use Cases:** Frontend applications, analytics dashboards, integration partners

---

### [Kobe Core](./core/README.md)
**Shared library** containing common data models, database schemas, utility functions, and business logic used across all services.

**Components:**
- Database models and schemas
- RPC utilities and helpers
- Validator app configurations
- Shared constants and types

**Use Cases:** Foundation for all other crates, ensures consistency across services

---

### [Kobe Cranker](./cranker/README.md)
**Automated stake pool management** service that executes critical epoch-boundary operations to maintain stake pool health and performance.

**Operations:**
- Epoch transition handling
- Stake pool state synchronization
- Performance metrics reporting

**Use Cases:** Essential for JitoSOL operations, reduces manual intervention, ensures pool reliability

---

### [Kobe Writer Service](./writer-service/README.md)
**Primary data collection** service that monitors Solana blockchain for MEV and priority fee events, processing and storing them in MongoDB.

**Capabilities:**
- Real-time blockchain monitoring
- MEV tip distribution tracking
- Priority fee reward processing
- Historical data backfilling

**Use Cases:** Powers all API endpoints, provides foundation for analytics and reporting

---

### [Kobe Steward Writer Service](./steward-writer-service/README.md)
**Specialized monitoring** service for Jito Steward program events, providing complete transparency into automated validator management decisions.

**Event Types:**
- Validator additions and removals
- Performance scoring and evaluation
- Risk management actions
- Stake rebalancing operations

**Use Cases:** Steward transparency, audit trails, performance analysis, regulatory compliance

## Quick Start

### Prerequisites

- Rust 1.85
- MongoDB 8.0
- Solana CLI tools
- Access to Solana RPC endpoints

### Environment Setup

```bash
# Clone the repository
git clone https://github.com/jito-foundation/kobe.git
cd kobe

# Set up environment variables
cp .env.example .env
# Edit .env with your configuration

# Build all crates
cargo build --release
```

### Running Services

#### Start the API Server

```bash
cargo r --bin kobe-api -- \
    --ip 127.0.0.1 \
    --port 8080 \
    --mongo-connection-uri "mongodb://<username>:<password>@localhost:27017" \
    --mongo-db-name validators \
    --sentry-api-url "" \
    --rpc-url "https://api.testnet.solana.com"
```

#### Start Cranker

```bash
RUST_LOG=info cargo r -p kobe-cranker -- \
    --fee-payer ~/.config/solana/id.json \
    --url "" \
    --network "testnet" \
    --pool-address "Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb" \
    --sentry-api-url "" \
    --region "testnet"
```

#### Start Data Collection

```bash
cargo r --bin kobe-writer-service -- live
```

#### Start Steward Monitoring

```bash
RUST_LOG=info cargo r -p kobe-steward-writer-service -- \
    --mongo-connection-uri "mongodb://localhost:27017/kobe" \
    --mongo-db-name "validators" \
    --rpc-url "" \
    --program-id "Stewardf95sJbmtcZsyagb2dg4Mo8eVQho8gpECvLx8" \
    --stake-pool "Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb" \
    listen
```

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

---

*Built with ❤️ by the Jito Foundation team*
