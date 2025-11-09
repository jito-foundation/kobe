use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Arc,
};

use anchor_lang::AccountDeserialize;
use axum::{http::StatusCode, Extension, Json};
use cached::{proc_macro::cached, TimedCache};
use kobe_core::{
    constants::{JITOSOL_VALIDATOR_LIST_MAINNET, JITOSOL_VALIDATOR_LIST_TESTNET},
    db_models::{
        mev_rewards::{StakerRewardsStore, ValidatorRewardsStore},
        stake_pool_stats::{StakePoolStats, StakePoolStatsStore},
        steward_events::StewardEventsStore,
        validators::ValidatorStore,
    },
    validators_app::Cluster,
    SortOrder, LAMPORTS_PER_SOL,
};
use log::{error, warn};
use mongodb::Database;
use serde::{Deserialize, Serialize};
use solana_borsh::v1::try_from_slice_unchecked;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_pubkey::Pubkey;
use spl_stake_pool::state::ValidatorList;
use stakenet_sdk::utils::accounts::get_validator_history_address;
use validator_history::ValidatorHistory;

use crate::{
    resolvers::error::{QueryResolverError, Result},
    schemas::{
        jitosol_ratio::{JitoSolRatioRequest, JitoSolRatioResponse},
        mev_rewards::{
            MevRewards, MevRewardsRequest, StakerRewards, StakerRewardsRequest,
            StakerRewardsResponse, ValidatorRewards, ValidatorRewardsRequest,
            ValidatorRewardsResponse,
        },
        stake_pool_stats::{
            round_to_hour, F64DataPoint, GetStakePoolStatsRequest, GetStakePoolStatsResponse,
            I64DataPoint,
        },
        steward_events::{StewardEvent, StewardEventsRequest, StewardEventsResponse},
        validator::{
            AverageMevCommissionOverTimeResponse, JitoStakeOverTimeResponse,
            ValidatorByVoteAccountResponse, ValidatorEntry, ValidatorsRequest, ValidatorsResponse,
        },
        validator_history::{EpochQuery, ValidatorHistoryEntryResponse, ValidatorHistoryResponse},
    },
};

#[derive(Clone)]
pub struct QueryResolver {
    stake_pool_store: StakePoolStatsStore,
    validator_store: ValidatorStore,
    validator_rewards_store: ValidatorRewardsStore,
    staker_rewards_store: StakerRewardsStore,
    steward_events_store: StewardEventsStore,

    /// RPC Client URL
    rpc_client: Arc<RpcClient>,

    /// Solana Cluster
    cluster: Cluster,
}

fn aggregate_mev_rewards(stats_entries: &[StakePoolStats]) -> u64 {
    /*
    We can have multiple stats entries per epoch, but each entry gives the cumulative MEV revenue up to that point,
    so we must dedup the entries from each epoch to get an accurate sum.
    */
    let mut deduped: HashMap<u64, &StakePoolStats> = HashMap::new();
    for entry in stats_entries {
        if !deduped.contains_key(&entry.epoch)
            || deduped[&entry.epoch].mev_rewards < entry.mev_rewards
        {
            deduped.insert(entry.epoch, entry);
        }
    }
    deduped.values().map(|v| v.mev_rewards).sum()
}

// Cache with 60 second lifespan
// If no request body, uses default, 7 days of data.
#[cached(
    type = "TimedCache<String, (StatusCode, Json<GetStakePoolStatsResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("stake-pool-{}", stats_request.to_string()) }"#
)]
pub async fn stake_pool_stats_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
    stats_request: GetStakePoolStatsRequest,
) -> (StatusCode, Json<GetStakePoolStatsResponse>) {
    if let Ok(stats) = resolver.get_stake_pool_stats(&stats_request).await {
        (StatusCode::OK, Json(stats))
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(GetStakePoolStatsResponse::default()),
        )
    }
}

// Cache with a 1 hour lifespan, using a fixed key since there's no varying input
#[cached(
    type = "TimedCache<String, (StatusCode, Json<AverageMevCommissionOverTimeResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(3600, 100) }",
    key = "String",
    convert = r#"{ "running-mev-commission-all-time".to_string() }"#
)]
pub async fn mev_commission_average_over_time_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
) -> (StatusCode, Json<AverageMevCommissionOverTimeResponse>) {
    match resolver.get_mev_commission_average_over_time().await {
        Ok(item) => (StatusCode::OK, Json(item)),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AverageMevCommissionOverTimeResponse::default()),
        ),
    }
}

// Cache with a 1 hour lifespan, using a fixed key since there's no varying input
#[cached(
    type = "TimedCache<String, (StatusCode, Json<JitoStakeOverTimeResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(3600, 100) }",
    key = "String",
    convert = r#"{ "running-jito-stake-all-time".to_string() }"#
)]
pub async fn jito_stake_over_time_ratio_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
) -> (StatusCode, Json<JitoStakeOverTimeResponse>) {
    match resolver.get_jito_stake_over_time_ratio().await {
        Ok(item) => (StatusCode::OK, Json(item)),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(JitoStakeOverTimeResponse::default()),
        ),
    }
}

// Cache with 60 second lifespan
// If no request body, uses cache string "validators-", which stores most recent epoch results.
#[cached(
    type = "TimedCache<String, (StatusCode, Json<ValidatorsResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("validators-{}", req.as_ref().map(|s| s.to_string()).unwrap_or_default()) }"#
)]
pub async fn validators_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
    req: Option<ValidatorsRequest>,
) -> (StatusCode, Json<ValidatorsResponse>) {
    if let Ok(res) = resolver.get_validators(&req).await {
        (StatusCode::OK, Json(res))
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ValidatorsResponse::default()),
        )
    }
}

#[cached(
    type = "TimedCache<String, (StatusCode, Json<ValidatorsResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("jitosol-validators-{}", req.as_ref().map(|s| s.to_string()).unwrap_or_default()) }"#
)]
pub async fn jitosol_validators_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
    req: Option<ValidatorsRequest>,
) -> (StatusCode, Json<ValidatorsResponse>) {
    if let Ok(res) = resolver.get_jitosol_validators(&req).await {
        (StatusCode::OK, Json(res))
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ValidatorsResponse::default()),
        )
    }
}

#[cached(
    type = "TimedCache<String, (StatusCode, Json<Vec<ValidatorByVoteAccountResponse>>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("validator-by-vote-account-{}", vote_account).to_string() }"#
)]
pub async fn validator_by_vote_account_cacheable_wrapper(
    vote_account: &String,
    resolver: Extension<QueryResolver>,
) -> (StatusCode, Json<Vec<ValidatorByVoteAccountResponse>>) {
    if let Ok(res) = resolver.get_validator(vote_account).await {
        (StatusCode::OK, Json(res))
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(vec![]))
    }
}

#[cached(
    type = "TimedCache<String, (StatusCode, Json<MevRewards>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("mev-rewards-{}", req.as_ref().map(|s| s.to_string()).unwrap_or_default()) }"#
)]
pub async fn mev_rewards_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
    req: Option<MevRewardsRequest>,
) -> (StatusCode, Json<MevRewards>) {
    if let Ok(res) = resolver.get_mev_rewards(&req).await {
        (StatusCode::OK, Json(res))
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(MevRewards::default()),
        )
    }
}

#[cached(
    type = "TimedCache<String, (StatusCode, Json<Vec<Row>>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(3600, 10) }",
    key = "String",
    convert = r#"{ "daily-mev-rewards".to_string() }"#
)]
pub async fn daily_mev_rewards_cacheable_wrapper() -> (StatusCode, Json<Vec<Row>>) {
    let dune_api_key = std::env::var("DUNE_API_KEY").unwrap_or_default();
    if let Ok(res) = get_daily_mev_rewards(&dune_api_key).await {
        (StatusCode::OK, Json(res))
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(vec![]))
    }
}

#[cached(
    type = "TimedCache<String, (StatusCode, Json<StewardEventsResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("steward-events-{}", req.to_string()) }"#
)]
pub async fn steward_events_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
    req: StewardEventsRequest,
) -> (StatusCode, Json<StewardEventsResponse>) {
    let offset = match (req.page, req.limit) {
        (Some(page), Some(limit)) => Some((page - 1) * limit),
        _ => None,
    };
    match resolver
        .get_steward_events(
            req.event_type,
            req.vote_account,
            req.epoch,
            offset,
            req.limit,
        )
        .await
    {
        Ok(res) => (StatusCode::OK, Json(StewardEventsResponse { events: res })),
        Err(e) => {
            error!("Error fetching steward events: {e:?}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StewardEventsResponse { events: vec![] }),
            )
        }
    }
}

#[cached(
    type = "TimedCache<String, (StatusCode, Json<ValidatorRewardsResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("validator-rewards-{}", req.to_string()) }"#
)]
pub async fn validator_rewards_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
    req: ValidatorRewardsRequest,
) -> (StatusCode, Json<ValidatorRewardsResponse>) {
    match resolver
        .get_validator_rewards(
            req.vote_account,
            req.epoch,
            req.page,
            req.limit.map(|l| l as i64),
            req.sort_order,
        )
        .await
    {
        Ok(res) => (StatusCode::OK, Json(res)),
        Err(e) => {
            error!("Error fetching validator rewards: {e:?}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ValidatorRewardsResponse::default()),
            )
        }
    }
}

#[cached(
    type = "TimedCache<String, (StatusCode, Json<StakerRewardsResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("staker-rewards-{}", req.to_string()) }"#
)]
pub async fn staker_rewards_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
    req: StakerRewardsRequest,
) -> (StatusCode, Json<StakerRewardsResponse>) {
    match resolver
        .get_staker_rewards(
            req.stake_authority,
            req.validator_vote_account,
            req.epoch,
            req.page,
            req.limit.map(|l| l as i64),
            req.sort_order,
        )
        .await
    {
        Ok(res) => (StatusCode::OK, Json(res)),
        Err(e) => {
            error!("Error fetching staker rewards: {e:?}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StakerRewardsResponse::default()),
            )
        }
    }
}

#[cached(
    type = "TimedCache<String, (StatusCode, Json<JitoSolRatioResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("jitosol-ratio-{}", req.as_ref().map(|s| s.to_string()).unwrap_or_default()) }"#
)]
pub async fn jitosol_ratio_cacheable_wrapper(
    resolver: Extension<QueryResolver>,
    req: Option<JitoSolRatioRequest>,
) -> (StatusCode, Json<JitoSolRatioResponse>) {
    if let Ok(res) = resolver.get_jitosol_ratio(&req).await {
        (StatusCode::OK, Json(res))
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(JitoSolRatioResponse::default()),
        )
    }
}

#[cached(
    type = "TimedCache<String, (StatusCode, Json<ValidatorHistoryResponse>)>",
    create = "{ TimedCache::with_lifespan_and_capacity(60, 1000) }",
    key = "String",
    convert = r#"{ format!("validator-history-{}-{}", vote_account, epoch.epoch.as_ref().map(|e| e.to_string()).unwrap_or(0.to_string())) }"#
)]
pub async fn get_validator_histories_wrapper(
    resolver: Extension<QueryResolver>,
    vote_account: String,
    epoch: EpochQuery,
) -> (StatusCode, Json<ValidatorHistoryResponse>) {
    if let Ok(res) = resolver.get_validator_histories(vote_account, epoch).await {
        (StatusCode::OK, Json(res))
    } else {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ValidatorHistoryResponse::default()),
        )
    }
}

impl QueryResolver {
    pub fn new(database: &Database, rpc_client_url: String, cluster: Cluster) -> Self {
        let client = RpcClient::new(rpc_client_url);

        Self {
            stake_pool_store: StakePoolStatsStore::new(
                database.collection(StakePoolStatsStore::COLLECTION),
            ),
            validator_store: ValidatorStore::new(database.collection(ValidatorStore::COLLECTION)),
            validator_rewards_store: ValidatorRewardsStore::new(
                database.collection(ValidatorRewardsStore::COLLECTION),
            ),
            staker_rewards_store: StakerRewardsStore::new(
                database.collection(StakerRewardsStore::COLLECTION),
            ),
            steward_events_store: StewardEventsStore::new(
                database.collection(StewardEventsStore::COLLECTION),
            ),
            rpc_client: Arc::new(client),
            cluster,
        }
    }

    pub async fn get_stake_pool_stats(
        &self,
        req: &GetStakePoolStatsRequest,
    ) -> Result<GetStakePoolStatsResponse> {
        let start = round_to_hour(req.range_filter.start);
        let end = round_to_hour(req.range_filter.end);
        let docs = self.stake_pool_store.aggregate(start, end).await?;

        let mut resp = GetStakePoolStatsResponse::default();
        for doc in docs.iter() {
            resp.apy.push(F64DataPoint {
                data: doc.apy,
                date: doc.timestamp,
            });
            resp.tvl.push(I64DataPoint {
                data: doc.total_solana_lamports as i64,
                date: doc.timestamp,
            });
            resp.mev_rewards.push(I64DataPoint {
                data: doc.mev_rewards as i64,
                date: doc.timestamp,
            });
            resp.num_validators.push(I64DataPoint {
                data: doc.num_validators as i64,
                date: doc.timestamp,
            });
            resp.supply.push(F64DataPoint {
                data: doc.total_pool_lamports as f64 / LAMPORTS_PER_SOL as f64,
                date: doc.timestamp,
            })
        }
        resp.aggregated_mev_rewards = aggregate_mev_rewards(docs.as_slice()) as i64;

        Ok(resp)
    }

    pub async fn get_mev_commission_average_over_time(
        &self,
    ) -> Result<AverageMevCommissionOverTimeResponse> {
        let average_mev_commission_over_time = self
            .validator_store
            .get_mev_commission_average_by_epoch()
            .await?;
        Ok(AverageMevCommissionOverTimeResponse {
            average_mev_commission_over_time,
        })
    }

    pub async fn get_jito_stake_over_time_ratio(&self) -> Result<JitoStakeOverTimeResponse> {
        let stake_ratio_over_time = self.validator_store.get_total_jito_stake_by_epoch().await?;
        Ok(JitoStakeOverTimeResponse {
            stake_ratio_over_time,
        })
    }

    pub async fn get_validators(
        &self,
        req: &Option<ValidatorsRequest>,
    ) -> Result<ValidatorsResponse> {
        let epoch = if let Some(request) = req {
            request.epoch
        } else {
            self.validator_store.get_highest_epoch().await?
        };

        let mev_rewards = self
            .validator_rewards_store
            .get_mev_rewards_per_validator(epoch)
            .await?;

        let validators = self.validator_store.find(epoch, true).await?;

        let response = ValidatorsResponse {
            validators: validators
                .into_iter()
                .map(|v| {
                    let mev_rewards = mev_rewards.get(&v.vote_account).unwrap_or(&0);

                    ValidatorEntry {
                        identity_account: v.identity_account,
                        active_stake: v.active_stake.unwrap_or(0),
                        vote_account: v.vote_account,
                        mev_commission_bps: if v.running_jito {
                            v.mev_commission_bps
                        } else {
                            None
                        },
                        mev_rewards: Some(*mev_rewards),
                        priority_fee_commission_bps: v.priority_fee_commission_bps,
                        priority_fee_rewards: v.priority_fee_revenue_lamports,
                        running_jito: v.running_jito,
                        running_bam: v.running_bam,
                        jito_sol_active_lamports: None,
                        jito_pool_eligible: v.jito_pool_eligible,
                        jito_pool_directed_stake_target: v.jito_directed_stake_target,
                    }
                })
                .collect(),
        };

        Ok(response)
    }

    pub async fn get_jitosol_validators(
        &self,
        req: &Option<ValidatorsRequest>,
    ) -> Result<ValidatorsResponse> {
        let epoch = if let Some(request) = req {
            request.epoch
        } else {
            self.validator_store.get_highest_epoch().await?
        };

        let mev_rewards = self
            .validator_rewards_store
            .get_mev_rewards_per_validator(epoch)
            .await?;

        let jito_sol_validator_list_address = match self.cluster {
            Cluster::MainnetBeta => JITOSOL_VALIDATOR_LIST_MAINNET,
            Cluster::Testnet => JITOSOL_VALIDATOR_LIST_TESTNET,
            Cluster::Devnet => {
                return Err(QueryResolverError::InvalidRequest(
                    "Devnet is not supported yet".to_string(),
                ));
            }
        };
        let jitosol_validator_list = Pubkey::from_str(jito_sol_validator_list_address)
            .map_err(|e| QueryResolverError::CustomError(e.to_string()))?;

        // Get stake pool validator list
        let validator_list_account = self
            .rpc_client
            .get_account_data(&jitosol_validator_list)
            .await
            .map_err(|e| QueryResolverError::RpcError(e.to_string()))?;

        let validator_list =
            try_from_slice_unchecked::<ValidatorList>(validator_list_account.as_slice())
                .map_err(|e| QueryResolverError::CustomError(e.to_string()))?;

        let jitosol_validator_set = validator_list
            .validators
            .iter()
            .map(|v| v.vote_account_address.to_string())
            .collect::<HashSet<String>>();

        let validators = self
            .validator_store
            .find(epoch, false)
            .await?
            .into_iter()
            .filter(|v| jitosol_validator_set.contains(&v.vote_account))
            .collect::<Vec<_>>();

        let response = ValidatorsResponse {
            validators: validators
                .into_iter()
                .map(|v| {
                    let mev_rewards = mev_rewards.get(&v.vote_account).unwrap_or(&0);

                    ValidatorEntry {
                        identity_account: v.identity_account,
                        active_stake: v.active_stake.unwrap_or(0),
                        vote_account: v.vote_account,
                        mev_commission_bps: if v.running_jito {
                            v.mev_commission_bps
                        } else {
                            None
                        },
                        mev_rewards: Some(*mev_rewards),
                        priority_fee_commission_bps: v.priority_fee_commission_bps,
                        priority_fee_rewards: v.priority_fee_revenue_lamports,
                        running_jito: v.running_jito,
                        running_bam: v.running_bam,
                        jito_sol_active_lamports: Some(v.target_pool_active_lamports),
                        jito_pool_eligible: v.jito_pool_eligible,
                        jito_pool_directed_stake_target: v.jito_directed_stake_target,
                    }
                })
                .collect(),
        };

        Ok(response)
    }

    pub async fn get_validator(
        &self,
        vote_account: &String,
    ) -> Result<Vec<ValidatorByVoteAccountResponse>> {
        let (mev_rewards, _) = self
            .validator_rewards_store
            .get_validator_rewards(Some(vote_account), None, None, None, None)
            .await?;

        let res = mev_rewards
            .into_iter()
            .map(|v| ValidatorByVoteAccountResponse {
                mev_commission_bps: v.mev_commission,
                mev_rewards: v.mev_revenue,
                priority_fee_commission_bps: v.priority_fee_commission.unwrap_or(0),
                priority_fee_rewards: v.priority_fee_revenue.unwrap_or(0),
                epoch: v.epoch,
            })
            .collect();

        Ok(res)
    }

    pub async fn get_mev_rewards(&self, req: &Option<MevRewardsRequest>) -> Result<MevRewards> {
        let highest_epoch = self.validator_rewards_store.get_highest_epoch().await?;

        let epoch = if let Some(request) = req {
            let epoch = request.epoch;
            if epoch > highest_epoch {
                return Err(QueryResolverError::InvalidRequest(format!(
                    "epoch {epoch} is higher than highest epoch {highest_epoch}"
                )));
            }
            epoch
        } else {
            highest_epoch
        };

        let total_network_mev_lamports = self
            .validator_rewards_store
            .get_mev_rewards_sum(epoch)
            .await?;
        let jito_stake_weight_lamports = self.validator_store.get_total_stake(epoch).await?;
        let mev_reward_per_lamport = if jito_stake_weight_lamports == 0 {
            0.0
        } else {
            total_network_mev_lamports as f64 / jito_stake_weight_lamports as f64
        };

        Ok(MevRewards {
            epoch,
            total_network_mev_lamports,
            jito_stake_weight_lamports,
            mev_reward_per_lamport,
        })
    }

    pub async fn get_staker_rewards(
        &self,
        stake_authority: Option<String>,
        validator_vote_account: Option<String>,
        epoch: Option<u64>,
        page: Option<u32>,
        limit: Option<i64>,
        sort_order: Option<String>,
    ) -> Result<StakerRewardsResponse> {
        let skip = (page.unwrap_or(1) - 1) * (limit.unwrap_or(100) as u32);
        let limit = limit.unwrap_or(100);
        let sort_order = sort_order.map(|s| match s.to_lowercase().as_str() {
            "asc" => SortOrder::Asc,
            _ => SortOrder::Desc,
        });
        let (staker_rewards, total_count) = self
            .staker_rewards_store
            .get_staker_rewards(
                stake_authority.as_deref(),
                validator_vote_account.as_deref(),
                epoch,
                Some(skip as u64),
                Some(limit),
                sort_order,
            )
            .await?;

        let rewards = staker_rewards
            .into_iter()
            .map(|r| StakerRewards {
                claimant: r.claimant,
                stake_authority: r.stake_authority,
                withdraw_authority: r.withdraw_authority,
                validator_vote_account: r.validator_vote_account,
                claim_status_account: r.claim_status_account,
                priority_fee_claim_status_account: r.priority_fee_claim_status_account,
                epoch: r.epoch,
                amount: r.amount,
                priority_fee_amount: r.priority_fee_amount,
            })
            .collect();

        Ok(StakerRewardsResponse {
            rewards,
            total_count,
        })
    }

    pub async fn get_validator_rewards(
        &self,
        vote_account: Option<String>,
        epoch: Option<u64>,
        page: Option<u32>,
        limit: Option<i64>,
        sort_order: Option<String>,
    ) -> Result<ValidatorRewardsResponse> {
        let skip = (page.unwrap_or(1) - 1) * (limit.unwrap_or(100) as u32);
        let limit = limit.unwrap_or(100);

        let sort_order = sort_order.map(|s| match s.to_lowercase().as_str() {
            "asc" => SortOrder::Asc,
            _ => SortOrder::Desc,
        });

        let (validator_rewards, total_count) = self
            .validator_rewards_store
            .get_validator_rewards(
                vote_account.as_ref(),
                epoch,
                Some(skip as u64),
                Some(limit),
                sort_order,
            )
            .await?;

        let rewards = validator_rewards
            .into_iter()
            .map(|r| ValidatorRewards {
                vote_account: r.vote_account,
                mev_revenue: r.mev_revenue,
                mev_commission: r.mev_commission,
                num_stakers: r.num_stakers,
                epoch: r.epoch,
                claim_status_account: r.claim_status_account,
                priority_fee_commission: r.priority_fee_commission,
                priority_fee_revenue: r.priority_fee_revenue.unwrap_or(0),
            })
            .collect();

        Ok(ValidatorRewardsResponse {
            rewards,
            total_count,
        })
    }

    pub async fn get_steward_events(
        &self,
        event_type: Option<String>,
        vote_account: Option<String>,
        epoch: Option<u64>,
        page: Option<u32>,
        limit: Option<u32>,
    ) -> Result<Vec<StewardEvent>> {
        let limit = limit.unwrap_or(100);
        let page = page.unwrap_or(1);
        let offset = (page - 1) * limit;

        let events = self
            .steward_events_store
            .find_steward_events(event_type, vote_account, epoch, limit as i64, offset as u64)
            .await?;

        Ok(events.into_iter().map(|event| event.into()).collect())
    }

    pub async fn get_jitosol_ratio(
        &self,
        req: &Option<JitoSolRatioRequest>,
    ) -> Result<JitoSolRatioResponse> {
        let range_filter = req
            .as_ref()
            .map(|r| r.range_filter.clone())
            .unwrap_or_default();

        let start = round_to_hour(range_filter.start);
        let end = round_to_hour(range_filter.end);

        let docs = self.stake_pool_store.aggregate(start, end).await?;

        if docs.is_empty() {
            return Ok(JitoSolRatioResponse::default());
        }

        let mut ratios = Vec::new();

        for doc in docs.iter() {
            if doc.total_pool_lamports > 0 {
                let ratio = doc.total_solana_lamports as f64 / doc.total_pool_lamports as f64;
                ratios.push(F64DataPoint {
                    data: ratio,
                    date: doc.timestamp,
                });
            } else {
                warn!(
                    "Skipping ratio calculation for timestamp {} due to zero pool lamports",
                    doc.timestamp
                );
            }
        }

        // Sort ratios in chronological order -- oldest first
        ratios.sort_by(|a, b| a.date.cmp(&b.date));

        Ok(JitoSolRatioResponse { ratios })
    }

    /// Retrieves the history of a specific validator, based on the provided vote account and optional epoch filter.
    ///
    /// # Returns
    ///
    /// - `Ok(Json(history))`: A JSON response containing the validator history information. If the epoch filter is provided, it only returns the history for the specified epoch.
    ///
    /// # Example
    ///
    /// This endpoint can be used to fetch the history of a validator's performance over time, either for a specific epoch or for all recorded epochs:
    ///
    /// ```
    /// GET /validator_history/{vote_account}?epoch=800
    /// ```
    /// This request retrieves the history for the specified vote account, filtered by epoch 800.
    pub async fn get_validator_histories(
        &self,
        vote_account: String,
        epoch_query: EpochQuery,
    ) -> Result<ValidatorHistoryResponse> {
        let vote_account = Pubkey::from_str(&vote_account)
            .map_err(|e| QueryResolverError::CustomError(e.to_string()))?;
        let history_account =
            get_validator_history_address(&vote_account, &validator_history::id());
        let account = self
            .rpc_client
            .get_account(&history_account)
            .await
            .map_err(|e| QueryResolverError::RpcError(e.to_string()))?;
        let validator_history = ValidatorHistory::try_deserialize(&mut account.data.as_slice())
            .map_err(|e| {
                error!("error deserializing ValidatorHistory: {:?}", e);
                QueryResolverError::ValidatorHistoryError(
                    "Error parsing ValidatorHistory".to_string(),
                )
            })?;

        let history_entries: Vec<ValidatorHistoryEntryResponse> = match epoch_query.epoch {
            Some(epoch) => validator_history
                .history
                .arr
                .iter()
                .filter_map(|entry| {
                    if epoch == entry.epoch {
                        Some(ValidatorHistoryEntryResponse::from_validator_history_entry(
                            entry,
                        ))
                    } else {
                        None
                    }
                })
                .collect(),
            None => validator_history
                .history
                .arr
                .iter()
                .map(ValidatorHistoryEntryResponse::from_validator_history_entry)
                .filter(|history| history.epoch.ne(&u16::MAX))
                .collect(),
        };

        let history =
            ValidatorHistoryResponse::from_validator_history(validator_history, history_entries);

        Ok(history)
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ApiResponse {
    result: ResultPayload,
}

#[derive(Serialize, Deserialize, Debug, Default)]
struct ResultPayload {
    rows: Vec<Row>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct Row {
    day: String,
    count_mev_tips: i64,
    jito_tips: f64,
    tippers: i64,
    validator_tips: f64,
}

pub async fn get_daily_mev_rewards(dune_api_key: &str) -> Result<Vec<Row>> {
    let client = reqwest::Client::new();

    let body = client
        .get("https://api.dune.com/api/v1/query/3715528/results?limit=2000")
        .header("X-Dune-API-Key", dune_api_key)
        .send()
        .await?;
    let json: ApiResponse = body.json().await?;
    Ok(json.result.rows)
}
