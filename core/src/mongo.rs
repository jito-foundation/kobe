use std::collections::HashMap;

use futures::TryStreamExt;
use mongodb::{bson::doc, Database};

use crate::{constants::VALIDATOR_COLLECTION_NAME, db_models::validators::Validator};

type Error = Box<dyn std::error::Error>;

pub async fn read_validator_info(db: &Database, epoch: u64) -> Result<Vec<Validator>, Error> {
    let collection = db.collection::<Validator>(VALIDATOR_COLLECTION_NAME);
    let cursor = collection.find(doc! {"epoch": epoch as u32}).await.unwrap();
    // Fetches all validators even if there are multiple entries for this epoch
    let validators: Vec<Validator> = cursor.try_collect().await?;
    // Select the latest entry
    let mut map: HashMap<String, Validator> = HashMap::new();
    for v in validators.into_iter() {
        if let Some(entry) = map.get(&v.vote_account) {
            if entry.timestamp < v.timestamp {
                map.insert(v.vote_account.clone(), v);
            }
        } else {
            map.insert(v.vote_account.clone(), v);
        }
    }
    Ok(map.into_values().collect::<Vec<Validator>>())
}
