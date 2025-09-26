use mongodb::{bson::doc, options::IndexOptions, Collection, IndexModel};
use serde::Deserialize;
pub use solana_native_token::LAMPORTS_PER_SOL;

use crate::db_models::validators::Validator;

pub mod client_type;
pub mod constants;
pub mod db_models;
pub mod fetcher;
pub mod mongo;
pub mod rpc_utils;
pub mod validators_app;

pub async fn add_index(collection: &Collection<Validator>, key: &str) {
    let options = IndexOptions::builder().unique(true).build();
    let model = IndexModel::builder()
        .keys(doc! {key: 1})
        .options(options)
        .build();
    // need to check if index does not exist (on load)
    collection
        .create_index(model)
        .await
        .expect("add index failed");
}

#[derive(Clone, Copy, Eq, PartialEq, Deserialize, Debug)]
pub enum SortOrder {
    Asc,
    Desc,
}
