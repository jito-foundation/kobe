# Stake Pool Cranker Quick-start

Below are the steps to configuring and running the Stake Pool Cranker. We recommend running it as a docker container.

## Setup

### Credentials

In the root directory create a new folder named `credentials` and then populate it with a keypair. This is keypair that signs and pays for all transactions.

```bash
mkdir credentials
solana-keygen new -o ./credentials/keypair.json
```

### ENV

In the cranker directory create `.env` file

```bash
touch .env
```

Then copy file contents below into the cranker sub-directory dotenv file at `./cranker/.env`. This file will require additional configuration. You will need to include a `JSON_RPC_URL` that can handle getProgramAccounts calls.

```bash
POOL_ADDRESS=KobeQvrdg63Z4YXsxX7KT7aHa6sGHoqs6JLSBExZS5z
SENTRY_API_URL="YOUR SENTRY URL HERE"
CLUSTER=mainnet
REGION=local
RUST_LOG="info,solana_gossip=error,solana_metrics=info"
# Metrics upload influx server (optional)
SOLANA_METRICS_CONFIG=""
PROGRAM_ID=
SOLANA_CLUSTER=
CONFIG_FILE=
STAKER=
FEE_PAYER=
DRY_RUN=
SIMULATE=
VALIDATORS_APP_TOKEN=
MEV_REVENUE=
DELEGATION_STRATEGY=
DELEGATION_FILE=
DIRECT_DELEGATION_FILE=
SLACK_API_TOKEN=
JSON_RPC_URL=
TX_PROXY_URL=
IP=
PORT=
MONGO_CONNECTION_URI=
MONGO_DB_NAME=
DUNE_API_KEY=
RPC_URL=
STAKE_POOL=
GOOGLE_APPLICATION_CREDENTIALS=
```

## Running Docker image from source

Once the setup is complete use the following commands to run/manage the docker container:

> Note: We are running `Docker version 24.0.5, build ced0996`

### Start Docker

```bash
docker compose --env-file .env up -d --build  kobe-cranker --remove-orphans
```

### View Logs

```bash
docker logs kobe-cranker -f
```

### Stop Docker\*\*

```bash
docker stop kobe-cranker; docker rm kobe-cranker;
```

## Running as Binary

To run the keeper in terminal, build for release and run the program.

### Build for Release

```bash
cd cranker
cargo build --release
```

### Run Keeper

```bash
cd cranker
RUST_LOG=info ./target/release/kobe-cranker
```

To see all available parameters run:

```bash
cd cranker
RUST_LOG=info ./target/release/kobe-cranker -h
```


