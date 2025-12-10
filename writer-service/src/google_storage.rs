use kobe_core::constants::MAINNET_SNAPSHOT_BUCKET_URL;
use serde::Deserialize;

use crate::result::{AppError, Result};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleStorageBucketItems {
    pub items: Vec<GoogleStorageBucketFile>,
    pub next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleStorageBucketFile {
    pub name: String,
    pub media_link: String,
}

pub fn filter_file(
    response: &[GoogleStorageBucketFile],
    name: String,
    epoch: u64,
    server_name: String,
) -> Result<&GoogleStorageBucketFile> {
    response
        .iter()
        .find(|file| {
            let file_epoch = file
                .name
                .split('/')
                .next()
                .unwrap_or_default()
                .parse::<u64>()
                .unwrap_or_default();
            file.name.contains(&name)
                && file_epoch == epoch
                && file.name.contains(server_name.as_str())
        })
        .ok_or(AppError::FileNotFound(name))
}

pub async fn get_file_uris(epoch: u64, mainnet_gcp_server_name: &str) -> Result<(String, String)> {
    let mut all_items = vec![];
    let mut next_page_token = String::from("");
    let items: Vec<GoogleStorageBucketFile> = loop {
        let response: GoogleStorageBucketItems = reqwest::get(format!(
            "{MAINNET_SNAPSHOT_BUCKET_URL}?pageToken={next_page_token}"
        ))
        .await?
        .json()
        .await?;

        all_items.extend(response.items);

        match response.next_page_token {
            Some(token) => next_page_token = token,
            None => break all_items,
        }
    };

    let merkle_tree_entry = filter_file(
        &items,
        String::from("merkle-tree"),
        epoch,
        mainnet_gcp_server_name.to_string(),
    )
    .map_err(|e| {
        AppError::FileNotFound(format!(
            "Failed to find merkle-tree file of epoch {epoch}: {e}"
        ))
    })?;

    let stake_meta_entry = filter_file(
        &items,
        String::from("stake-meta"),
        epoch,
        mainnet_gcp_server_name.to_string(),
    )
    .map_err(|e| {
        AppError::FileNotFound(format!(
            "Failed to find stake-meta file of epoch {epoch}: {e}"
        ))
    })?;

    Ok((
        merkle_tree_entry.media_link.to_owned(),
        stake_meta_entry.media_link.to_owned(),
    ))
}
