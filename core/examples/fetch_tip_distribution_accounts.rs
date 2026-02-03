use clap::Parser;
use kobe_core::{
    fetcher::fetch_tip_distribution_accounts,
    validators_app::{Cluster, ValidatorsAppResponseEntry},
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_pubkey::Pubkey;

#[derive(Parser)]
#[command(about = "Test fetching tip distribution accounts")]
struct Args {
    /// RPC URL
    #[arg(long)]
    rpc_url: String,

    /// Solana cluster (e.g. mainnet, testnet, devnet)
    #[arg(long)]
    cluster: String,

    /// Epoch number
    #[arg(long)]
    epoch: u64,

    /// Vote account
    #[arg(long)]
    vote_account: Pubkey,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let cluster = Cluster::get_cluster(&args.cluster).unwrap();
    let rpc_client = RpcClient::new(args.rpc_url);

    let mut entry = ValidatorsAppResponseEntry::default();
    entry.vote_account = args.vote_account;

    let accounts = fetch_tip_distribution_accounts(&[entry], &rpc_client, &cluster, args.epoch)
        .await
        .unwrap();

    println!("Accounts: {}", accounts.len());
}
