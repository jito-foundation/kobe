use std::time::Duration;

use kobe_api_client::client_builder::KobeApiClientBuilder;

#[tokio::main]
async fn main() {
    let client = KobeApiClientBuilder::new()
        .timeout(Duration::from_secs(45))
        .retry(true)
        .max_retries(5)
        .build();

    let current_epoch = client.get_current_epoch().await.unwrap();
    println!("Current epoch: {}\n", current_epoch);

    let validators_res = client.get_validators(Some(current_epoch)).await.unwrap();

    println!("Found {} validators", validators_res.validators.len());

    let first_validator = validators_res.validators[0].clone();
    println!("Is running bam: {:?}", first_validator.running_bam);
    println!("Is eligible: {:?}", first_validator.jito_pool_eligible);
    println!(
        "Is directed stake target: {:?}",
        first_validator.jito_pool_directed_stake_target
    );
}
