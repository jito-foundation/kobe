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

fn filter_file(
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

pub async fn get_file_uris(
    epoch: u64,
    mainnet_gcp_server_names: &[String],
) -> Result<(String, String)> {
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

    let merkle_tree_entry = mainnet_gcp_server_names
        .iter()
        .find_map(|gcp_name| {
            filter_file(
                &items,
                String::from("merkle-tree"),
                epoch,
                gcp_name.to_owned(),
            )
            .ok()
        })
        .ok_or_else(|| {
            AppError::FileNotFound(format!("Failed to find merkle-tree file of epoch {epoch}"))
        })?;

    let stake_meta_entry = mainnet_gcp_server_names
        .iter()
        .find_map(|gcp_name| {
            filter_file(
                &items,
                String::from("stake-meta"),
                epoch,
                gcp_name.to_owned(),
            )
            .ok()
        })
        .ok_or_else(|| {
            AppError::FileNotFound(format!("Failed to find stake-meta file of epoch {epoch}"))
        })?;

    Ok((
        merkle_tree_entry.media_link.to_owned(),
        stake_meta_entry.media_link.to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use crate::google_storage::{filter_file, get_file_uris, GoogleStorageBucketFile};

    #[test]
    fn filter_file_success() {
        let response = vec![GoogleStorageBucketFile {
            name: "844/tip-router-rpc-1/844-merkle-tree-collection.json".to_string(),
            media_link: "https://storage.googleapis.com/download/storage/v1/b/jito-mainnet/o/844%2Ftip-router-rpc-1%2F844-merkle-tree-collection.json?generation=1757167743549992&alt=media".to_string()
        }];
        let name = "merkle-tree".to_string();
        let epoch = 844;
        let server_name = "tip-router-rpc-1".to_string();

        let bucket_file = filter_file(&response, name, epoch, server_name);

        assert!(bucket_file.is_ok());
    }

    #[tokio::test]
    async fn get_file_uris_success() {
        let epoch = 844;
        let mainnet_gcp_server_names = vec![
            String::from("tip-router-rpc-1"),
            String::from("tip-router-rpc-2"),
            String::from("tip-router-rpc-3"),
        ];

        let file_uris = get_file_uris(epoch, &mainnet_gcp_server_names).await;

        assert!(file_uris.is_ok());

        let file_uris = file_uris.unwrap();

        assert_eq!(file_uris.0, "https://storage.googleapis.com/download/storage/v1/b/jito-mainnet/o/844%2Ftip-router-rpc-1%2F844-merkle-tree-collection.json?generation=1757167743549992&alt=media");
    }
}
