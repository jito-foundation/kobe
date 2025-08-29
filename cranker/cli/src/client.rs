use solana_client::rpc_client::RpcClient;
use solana_program::borsh1::try_from_slice_unchecked;
use solana_program::{program_pack::Pack, pubkey::Pubkey};
use spl_stake_pool::state::{StakePool, ValidatorList};

type Error = Box<dyn std::error::Error>;

pub async fn get_stake_pool(
    rpc_client: &solana_client::nonblocking::rpc_client::RpcClient,
    stake_pool_address: &Pubkey,
) -> Result<StakePool, Error> {
    let account_data = rpc_client.get_account_data(stake_pool_address).await?;
    let stake_pool = try_from_slice_unchecked::<StakePool>(account_data.as_slice())
        .map_err(|err| format!("Invalid stake pool {stake_pool_address}: {err}"))?;
    Ok(stake_pool)
}

pub async fn get_validator_list(
    rpc_client: &solana_client::nonblocking::rpc_client::RpcClient,
    validator_list_address: &Pubkey,
) -> Result<ValidatorList, Error> {
    let account_data = rpc_client.get_account_data(validator_list_address).await?;
    let validator_list = try_from_slice_unchecked::<ValidatorList>(account_data.as_slice())
        .map_err(|err| format!("Invalid validator list {validator_list_address}: {err}"))?;
    Ok(validator_list)
}

pub fn get_token_account(
    rpc_client: &RpcClient,
    token_account_address: &Pubkey,
    expected_token_mint: &Pubkey,
) -> Result<spl_token::state::Account, Error> {
    let account_data = rpc_client.get_account_data(token_account_address)?;
    let token_account = spl_token::state::Account::unpack_from_slice(account_data.as_slice())
        .map_err(|err| format!("Invalid token account {token_account_address}: {err}"))?;

    if token_account.mint != *expected_token_mint {
        Err(format!(
            "Invalid token mint for {token_account_address}, expected mint is {expected_token_mint}"
        )
        .into())
    } else {
        Ok(token_account)
    }
}

pub fn get_token_mint(
    rpc_client: &RpcClient,
    token_mint_address: &Pubkey,
) -> Result<spl_token::state::Mint, Error> {
    let account_data = rpc_client.get_account_data(token_mint_address)?;
    let token_mint = spl_token::state::Mint::unpack_from_slice(account_data.as_slice())
        .map_err(|err| format!("Invalid token mint {token_mint_address}: {err}"))?;

    Ok(token_mint)
}
