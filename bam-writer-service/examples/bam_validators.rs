use reqwest::Client;
use serde::Deserialize;

const BASE_URL: &str = "https://kobe.mainnet.jito.network";

#[derive(Deserialize)]
struct BamValidatorsResponse {
    bam_validators: Vec<BamValidator>,
}

#[derive(Deserialize)]
struct BamValidator {
    is_eligible: bool,
    vote_account: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let epoch = 950;
    let url = format!("{BASE_URL}/api/v1/bam_validators?epoch={epoch}");

    println!("Fetching BAM validators for epoch {epoch}...");

    let client = Client::new();
    let resp: BamValidatorsResponse = client.get(&url).send().await?.json().await?;

    let total = resp.bam_validators.len();
    let eligible: Vec<_> = resp
        .bam_validators
        .iter()
        .filter(|v| v.is_eligible)
        .collect();

    println!("Total validators: {total}");
    println!("Eligible (is_eligible=true): {}", eligible.len());
    println!("Ineligible (is_eligible=false): {}", total - eligible.len());

    println!("\nEligible validators:");
    for v in &eligible {
        println!("  {}", v.vote_account);
    }

    Ok(())
}
