TAG=latest



# COLOR CONFIG
GREEN  := $(shell tput -Txterm setaf 2)
YELLOW := $(shell tput -Txterm setaf 3)
WHITE  := $(shell tput -Txterm setaf 7)
RESET  := $(shell tput -Txterm sgr0)


TARGET_MAX_CHAR_NUM=20
## Show help
help:
	@echo ''
	@echo 'Usage:'
	@echo '  CONFIG_PATH=<config_path> ${YELLOW}make${RESET} ${GREEN}<target>${RESET}'
	@echo ''
	@echo 'Targets:'
	@awk '/^[a-zA-Z\-\_0-9]+:/ { \
		helpMessage = match(lastLine, /^## (.*)/); \
		if (helpMessage) { \
			helpCommand = substr($$1, 0, index($$1, ":")-1); \
			helpMessage = substr(lastLine, RSTART + 3, RLENGTH); \
			printf "  ${YELLOW}%-$(TARGET_MAX_CHAR_NUM)s${RESET} ${GREEN}%s${RESET}\n", helpCommand, helpMessage; \
		} \
	} \
	{ lastLine = $$0 }' $(MAKEFILE_LIST)

## check that config file is loaded correctly
check-env:
	@if [ -z "$(CONFIG_PATH)" ]; then echo "env_file env var is not set. aborting." && exit 1; fi

## Build validator service
build:
	@echo "building validator services and cranker"
	git submodule update --init --recursive
	cargo build --release

## Run cargo tidy scripts
tidy:
	cargo sort --workspace
	cargo +nightly udeps
	cargo clippy --fix --allow-staged --allow-dirty

## Start local database
start-database-local:
	@echo "starting mongodb (docker)"
	 docker run --name mongodb -d -p 27017:27017 mongo

## Stop local database
stop-database-local:
	@echo "stopping mongodb as background process"
	docker stop mongodb

## Run an instance of the indexer
run-indexer:
	@echo "starting indexer"
	@echo $(MONGO_CONNECTION_URI)
	@echo $(MONGO_DB_NAME)
	@echo $(VALIDATORS_APP_TOKEN)
	VALIDATORS_APP_TOKEN=$(VALIDATORS_APP_TOKEN) ./target/release/kobe-writer-service --mongo-connection-uri "$(MONGO_CONNECTION_URI)" --mongo-db-name $(MONGO_DB_NAME)

## Run an instance of the cranker
run-cranker-dry:
	@echo "starting a dry run of the Cranker (Unimplemented)"

## Start graphql service
start-graphql:		## start graphql service
	@echo "starting graphql service on cluster $(SOLANA_CLUSTER)"
	RUST_LOG=info cargo run --manifest-path api/Cargo.toml --  \
 	--ip $(GRAPHQL_IP) --port $(GRAPHQL_PORT) --mongo-connection-uri $(MONGO_CONNECTION_URI) --mongo-db-name $(MONGO_DB_NAME) --solana-cluster $(SOLANA_CLUSTER)

## Run all docker containers in the foreground
run-docker: check-env
	@echo "starting docker containers"
	@echo $(CONFIG_PATH)
	COMPOSE_DOCKER_CLI_BUILD=1 DOCKER_BUILDKIT=1 docker compose --env-file $(CONFIG_PATH) up --build --remove-orphans

## Check docker env variables, given a loaded config file
check-docker: check-env
	@echo "checking docker env variables..."
	docker compose --env-file $(CONFIG_PATH) config

## Build and start rest api in background, restarting if exists
start-api: check-env
	@echo "restarting api"
	@echo $(CONFIG_PATH)
	COMPOSE_DOCKER_CLI_BUILD=1 DOCKER_BUILDKIT=1 docker compose --env-file $(CONFIG_PATH) up -d --build --remove-orphans api-mainnet

## Build and start rest api in background, restarting if exists
start-api-testnet: check-env
	@echo "restarting api"
	@echo $(CONFIG_PATH)
	COMPOSE_DOCKER_CLI_BUILD=1 DOCKER_BUILDKIT=1 docker compose --env-file $(CONFIG_PATH) up -d --build --remove-orphans api-testnet


## Build and start db writer in background, restarting if exists
start-db-writer: check-env
	@echo "restarting db writer"
	@echo $(CONFIG_PATH)
	COMPOSE_DOCKER_CLI_BUILD=1 DOCKER_BUILDKIT=1 docker compose --env-file $(CONFIG_PATH) up -d --build --remove-orphans writer-service-mainnet

make start-testnet-db-writer: check-env
	@echo "restarting testnet writer"
	@echo $(CONFIG_PATH)
	COMPOSE_DOCKER_CLI_BUILD=1 DOCKER_BUILDKIT=1 docker compose -p testnet --env-file $(CONFIG_PATH) up -d --build --remove-orphans writer-service-testnet

## Build and start cranker in background, restarting if exists
start-cranker: check-env
	@echo "restarting cranker"
	@echo $(CONFIG_PATH)
	COMPOSE_DOCKER_CLI_BUILD=1 DOCKER_BUILDKIT=1 docker compose --env-file $(CONFIG_PATH) up -d --build --remove-orphans cranker


## Build and start steward writer service mainnet in background, restarting if exists
start-steward-writer-mainnet: check-env
	@echo "restarting steward writer mainnet"
	@echo $(CONFIG_PATH)
	COMPOSE_DOCKER_CLI_BUILD=1 DOCKER_BUILDKIT=1 docker compose --env-file $(CONFIG_PATH) up -d --build --remove-orphans steward-writer-service-mainnet

## Build and start steward writer service testnet in background, restarting if exists
start-steward-writer-testnet: check-env
	@echo "restarting steward writer testnet"
	@echo $(CONFIG_PATH)
	COMPOSE_DOCKER_CLI_BUILD=1 DOCKER_BUILDKIT=1 docker compose -p testnet --env-file $(CONFIG_PATH) up -d --build --remove-orphans steward-writer-service-testnet
