// use std::{
//     ops::{Add, Sub},
//     sync::Once,
// };
//
// use chrono::{Duration, SubsecRound, Utc};
// use kobe_core::db_models::stake_pool_stats::{StakePoolStats, StakePoolStatsStore};
// use mongodb::Client;
// use tokio::time::timeout;
//
// /// NOTE: Must have MongoDB running.
// const MONGO_CONNECTION_URI: &str = "mongodb://root:kakarat@127.0.0.1:27017";
// const DATABASE: &str = "test-db";
//
// static ENV: Once = Once::new();
// fn env_logger() {
//     ENV.call_once(|| {
//         env_logger::init();
//     });
// }
//
// #[tokio::test]
// async fn test_aggregate_stake_pool_stats_happy_path() {
//     env_logger();
//
//     let c = timeout(
//         std::time::Duration::from_secs(5),
//         Client::with_uri_str(MONGO_CONNECTION_URI),
//     )
//     .await
//     .expect("timed out")
//     .expect("Mongo connection failed.");
//
//     let c = c
//         .database(DATABASE)
//         .collection(StakePoolStatsStore::COLLECTION);
//     timeout(std::time::Duration::from_secs(1), c.drop(None))
//         .await
//         .expect("timed out. is mongo running?")
//         .unwrap();
//
//     let today = Utc::now();
//     let one_day_past = today.sub(Duration::days(1));
//     let two_days_past = today.sub(Duration::days(2));
//     let one_day_future = today.add(Duration::days(1));
//     let half_day_future = today.add(Duration::hours(12));
//     let half_day_past = today.sub(Duration::hours(12));
//     let two_days_future = today.add(Duration::days(2));
//
//     // Setup
//     let docs = vec![
//         StakePoolStats {
//             epoch: 1,
//             num_deposits: 100000,
//             reserve_balance: 43781,
//             timestamp: today,
//             total_solana_lamports: 1223,
//             total_pool_lamports: 4522,
//             mev_rewards: 12353,
//             apy: 12.3,
//             num_validators: 12,
//             fees_collected: Some(0.0),
//             total_network_staked_lamports: Some(400_000_000 * 1_000_000_000),
//         },
//         StakePoolStats {
//             epoch: 1,
//             num_deposits: 340000,
//             reserve_balance: 53431,
//             timestamp: today,
//             total_solana_lamports: 4587,
//             total_pool_lamports: 2948,
//             mev_rewards: 4325,
//             apy: 5.99,
//             num_validators: 15,
//             fees_collected: Some(0.0),
//             total_network_staked_lamports: Some(400_000_000 * 1_000_000_000),
//         },
//         StakePoolStats {
//             epoch: 2,
//             num_deposits: 5454,
//             reserve_balance: 201,
//             timestamp: half_day_future,
//             total_solana_lamports: 483,
//             total_pool_lamports: 1254,
//             mev_rewards: 213,
//             apy: 7.99,
//             num_validators: 15,
//             fees_collected: Some(0.0),
//             total_network_staked_lamports: Some(400_000_000 * 1_000_000_000),
//         },
//         StakePoolStats {
//             epoch: 2,
//             num_deposits: 5454,
//             reserve_balance: 4000,
//             timestamp: half_day_past,
//             total_solana_lamports: 50,
//             total_pool_lamports: 1254,
//             mev_rewards: 213,
//             apy: 3.99,
//             num_validators: 100,
//             total_network_staked_lamports: Some(400_000_000 * 1_000_000_000),
//             fees_collected: Some(0.0),
//         },
//         StakePoolStats {
//             epoch: 1,
//             num_deposits: 347282,
//             reserve_balance: 3984,
//             timestamp: one_day_past,
//             total_solana_lamports: 768,
//             total_pool_lamports: 82834,
//             mev_rewards: 482,
//             apy: 8.99,
//             num_validators: 958,
//             fees_collected: Some(0.0),
//             total_network_staked_lamports: Some(400_000_000 * 1_000_000_000),
//         },
//         StakePoolStats {
//             epoch: 1,
//             num_deposits: 454777,
//             reserve_balance: 235555,
//             timestamp: one_day_past,
//             total_solana_lamports: 4365565,
//             total_pool_lamports: 23432,
//             mev_rewards: 43567625,
//             apy: 7.52,
//             num_validators: 18,
//             fees_collected: Some(0.0),
//             total_network_staked_lamports: Some(400_000_000 * 1_000_000_000),
//         },
//         StakePoolStats {
//             epoch: 2,
//             num_deposits: 5454,
//             reserve_balance: 201,
//             timestamp: one_day_future,
//             total_solana_lamports: 483,
//             total_pool_lamports: 1254,
//             mev_rewards: 213,
//             apy: 7.99,
//             num_validators: 15,
//             fees_collected: Some(0.0),
//             total_network_staked_lamports: Some(400_000_000 * 1_000_000_000),
//         },
//         StakePoolStats {
//             epoch: 0,
//             num_deposits: 4564,
//             reserve_balance: 9654,
//             timestamp: two_days_past,
//             total_solana_lamports: 775468,
//             total_pool_lamports: 33665,
//             mev_rewards: 112552,
//             apy: 7.52,
//             num_validators: 12,
//             fees_collected: Some(0.0),
//             total_network_staked_lamports: Some(400_000_000 * 1_000_000_000),
//         },
//         StakePoolStats {
//             epoch: 2,
//             num_deposits: 4520,
//             reserve_balance: 765,
//             timestamp: two_days_future,
//             total_solana_lamports: 759,
//             total_pool_lamports: 1212,
//             mev_rewards: 656,
//             apy: 1.99,
//             num_validators: 18,
//             fees_collected: Some(0.0),
//             total_network_staked_lamports: Some(400_000_000 * 1_000_000_000),
//         },
//     ];
//     c.insert_many(docs.clone(), None).await.unwrap();
//
//     let store = StakePoolStatsStore::new(c);
//     let resp = store.aggregate(one_day_past, one_day_future).await.unwrap();
//     assert_eq!(resp.len(), 3);
//
//     let mut last_today_doc = docs[1].clone();
//     last_today_doc.timestamp = last_today_doc.timestamp.trunc_subsecs(0);
//
//     // Half day past doc
//     let mut last_one_day_past_doc = docs[2].clone();
//     last_one_day_past_doc.timestamp = last_one_day_past_doc.timestamp.trunc_subsecs(0);
//
//     let mut last_one_day_future_doc = docs[6].clone();
//     last_one_day_future_doc.timestamp = last_one_day_future_doc.timestamp.trunc_subsecs(0);
//
//     assert_eq!(resp[0], last_one_day_past_doc);
//     assert_eq!(resp[1], last_today_doc);
//     assert_eq!(resp[2], last_one_day_future_doc);
// }
