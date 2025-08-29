use clap::Parser;
use kobe_core::validators_app::Cluster;
use solana_pubkey::Pubkey;

fn validate_network(network: &str) -> Result<String, String> {
    match network {
        "mainnet-beta" | "mainnet" | "m" | "testnet" | "t" => Ok(network.to_string()),
        _ => Err("Network must be testnet or mainnet-beta".to_string()),
    }
}

#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(about = "Stake pool management tool")]
pub struct Args {
    /// Configuration file to use
    #[arg(
        short = 'C',
        long = "config",
        value_name = "PATH",
        env = "CONFIG_FILE",
        global = true
    )]
    pub config_file: Option<String>,

    /// Stake pool address
    #[arg(
        long = "pool-address",
        value_name = "POOL_ADDRESS",
        env = "POOL_ADDRESS",
        required = true
    )]
    pub pool: Pubkey,

    /// Transaction fee payer account [default: cli config keypair]
    #[arg(long = "fee-payer", value_name = "KEYPAIR", env = "FEE_PAYER")]
    pub fee_payer: Option<String>,

    /// JSON RPC URL for the cluster. Default from the configuration file.
    #[arg(long = "url", value_name = "URL", env = "RPC_URL")]
    pub rpc_url: Option<String>,

    /// Dry run to see stake movements without executing transactions
    #[arg(long = "dry-run", env = "DRY_RUN")]
    pub dry_run: bool,

    /// Simulate to see success/failure of transactions without executing
    #[arg(long = "simulate", env = "SIMULATE")]
    pub simulate: bool,

    /// Network to use (testnet, mainnet-beta, mainnet, m, t)
    #[arg(
        long = "network",
        env = "SOLANA_CLUSTER",
        value_parser = validate_network,
        default_value = "mainnet-beta"
    )]
    pub network: String,

    /// Sentry API url
    #[arg(long = "sentry-api-url", env = "SENTRY_API_URL", required = true)]
    pub sentry_api_url: String,

    /// Slack bearer api token
    #[arg(long = "slack-api-token", env = "SLACK_API_TOKEN")]
    pub slack_api_token: Option<String>,

    /// Region label for metrics purposes
    #[arg(
        long = "region",
        env = "REGION",
        required = true,
        default_value = "local"
    )]
    pub region: String,
}

impl Args {
    /// Get the config file path, using the default if not specified
    pub fn get_config_file(&self) -> Option<String> {
        self.config_file.clone().or_else(|| {
            solana_cli_config::CONFIG_FILE
                .as_ref()
                .map(|config_file| config_file.clone())
        })
    }

    /// Parse the network into a Cluster enum
    pub fn get_cluster(&self) -> Cluster {
        match self.network.as_str() {
            "testnet" | "t" => Cluster::Testnet,
            "mainnet-beta" | "mainnet" | "m" => Cluster::MainnetBeta,
            _ => panic!("invalid cluster specified"), // This shouldn't happen due to validation
        }
    }

    /// Get the stake pool address, using defaults if needed
    pub fn get_stake_pool_address(&self) -> Pubkey {
        self.pool
    }

    /// Get the JSON RPC URL with cluster-based fallback
    pub fn get_json_rpc_url(&self) -> String {
        self.rpc_url
            .clone()
            .unwrap_or_else(|| match self.get_cluster() {
                Cluster::MainnetBeta => "https://api.mainnet-beta.solana.com".into(),
                Cluster::Testnet => "https://api.testnet.solana.com".into(),
            })
    }
}
