# Kobe BAM Writer Service

## Overview

Kobe BAM Writer Service is a specialized data collection service that monitors and records BAM delegations.

- BAM total network stake weight per-epoch
- available BAM delegation stake per-epoch (computed based on ^ refer to JIP-28)
- # of eligible validators per-epoch (delegation denominator)
- list of eligible validators
- list of directed stake targets (targets that bypass the ticket system)

BAM Epoch Metrics

- epoch
- total_network_stake_weight
- available_delegation_stake
- eligible_validator_count
- timestamp

BAM validators table -> (validators table)

- id
- epoch
- vote_account
- is_eligible
- is_directed_stake_target



## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](../LICENSE) file for details.

## Contribution

Contributions are welcome! Please feel free to submit a Pull Request.
