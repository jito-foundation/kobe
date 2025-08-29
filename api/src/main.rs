use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use axum::{
    error_handling::HandleErrorLayer,
    extract::{Path, Query},
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::get,
    Extension, Json, Router, Server,
};
use clap::Parser;
use env_logger::{Builder, Target};
use kobe_api::{
    error::{handle_error, ApiError},
    resolvers::query_resolver::{
        daily_mev_rewards_cacheable_wrapper, jito_stake_over_time_ratio_cacheable_wrapper,
        jitosol_ratio_cacheable_wrapper, jitosol_validators_cacheable_wrapper,
        mev_commission_average_over_time_cacheable_wrapper, mev_rewards_cacheable_wrapper,
        stake_pool_stats_cacheable_wrapper, staker_rewards_cacheable_wrapper,
        steward_events_cacheable_wrapper, validator_by_vote_account_cacheable_wrapper,
        validator_rewards_cacheable_wrapper, validators_cacheable_wrapper, QueryResolver,
    },
    schemas::{
        jitosol_ratio::JitoSolRatioRequest,
        mev_rewards::{MevRewardsRequest, StakerRewardsRequest, ValidatorRewardsRequest},
        stake_pool_stats::GetStakePoolStatsRequest,
        steward_events::StewardEventsRequest,
        validator::ValidatorsRequest,
    },
};
use kobe_core::db_models::mev_rewards::{StakerRewardsStore, ValidatorRewardsStore};
use log::*;
use mongodb::Client;
use serde_json::json;
use tower::{buffer::BufferLayer, limit::RateLimitLayer, timeout::TimeoutLayer, ServiceBuilder};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};

async fn stake_pool_stats_handler(
    resolver: Extension<QueryResolver>,
    request: Option<Json<GetStakePoolStatsRequest>>,
) -> Result<impl IntoResponse, ApiError> {
    // Note that JSON bodies on GET requests are dropped by cloudflare so all GETs will just use default args
    let Json(stats_request) = if let Some(json_request) = request {
        json_request
    } else {
        Json(GetStakePoolStatsRequest::default())
    };

    stats_request.validate()?;

    Ok(stake_pool_stats_cacheable_wrapper(resolver, stats_request).await)
}

async fn validators_handler(
    resolver: Extension<QueryResolver>,
    request: Option<Json<ValidatorsRequest>>,
) -> impl IntoResponse {
    // Note that JSON bodies on GET requests are dropped by cloudflare so all GETs will just use default args
    let req = if let Some(json_request) = request {
        Some(json_request.0)
    } else {
        None
    };
    validators_cacheable_wrapper(resolver, req).await
}

async fn jitosol_validators_handler(
    resolver: Extension<QueryResolver>,
    request: Option<Json<ValidatorsRequest>>,
) -> impl IntoResponse {
    // Note that JSON bodies on GET requests are dropped by cloudflare so all GETs will just use default args
    let req = if let Some(json_request) = request {
        Some(json_request.0)
    } else {
        None
    };
    jitosol_validators_cacheable_wrapper(resolver, req).await
}

async fn validator_by_vote_account_handler(
    Path(vote_account): Path<String>,
    resolver: Extension<QueryResolver>,
) -> impl IntoResponse {
    validator_by_vote_account_cacheable_wrapper(&vote_account, resolver).await
}

async fn mev_commission_average_over_time_handler(
    resolver: Extension<QueryResolver>,
) -> impl IntoResponse {
    mev_commission_average_over_time_cacheable_wrapper(resolver).await
}

async fn jito_stake_over_time_handler(resolver: Extension<QueryResolver>) -> impl IntoResponse {
    jito_stake_over_time_ratio_cacheable_wrapper(resolver).await
}

async fn mev_rewards_handler(
    resolver: Extension<QueryResolver>,
    request: Option<Json<MevRewardsRequest>>,
) -> impl IntoResponse {
    // Note that JSON bodies on GET requests are dropped by cloudflare so all GETs will just use default args
    let req = if let Some(json_request) = request {
        Some(json_request.0)
    } else {
        None
    };

    mev_rewards_cacheable_wrapper(resolver, req).await
}

#[allow(unused_variables)]
async fn daily_mev_rewards_handler(resolver: Extension<QueryResolver>) -> impl IntoResponse {
    daily_mev_rewards_cacheable_wrapper().await
}

async fn validator_rewards_handler(
    resolver: Extension<QueryResolver>,
    request: Query<ValidatorRewardsRequest>,
) -> impl IntoResponse {
    if let Some(limit) = request.limit {
        if limit > ValidatorRewardsStore::MAX_LIMIT {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Limit exceeds maximum allowed value of {}", ValidatorRewardsStore::MAX_LIMIT)
                })),
            )
                .into_response();
        }
    }

    validator_rewards_cacheable_wrapper(resolver, request.0)
        .await
        .into_response()
}

async fn staker_rewards_handler_v1(
    resolver: Extension<QueryResolver>,
    request: Query<StakerRewardsRequest>,
) -> impl IntoResponse {
    if let Some(limit) = request.limit {
        if limit > StakerRewardsStore::MAX_LIMIT {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("Limit exceeds maximum allowed value of {}", StakerRewardsStore::MAX_LIMIT)
                })),
            )
                .into_response();
        }
    }

    staker_rewards_cacheable_wrapper(resolver, request.0)
        .await
        .into_response()
}

async fn steward_events_handler(
    resolver: Extension<QueryResolver>,
    request: Query<StewardEventsRequest>,
) -> impl IntoResponse {
    steward_events_cacheable_wrapper(resolver, request.0).await
}

async fn jitosol_sol_ratio_handler(
    resolver: Extension<QueryResolver>,
    request: Option<Json<JitoSolRatioRequest>>,
) -> impl IntoResponse {
    let req = if let Some(json_request) = request {
        Some(json_request.0)
    } else {
        None
    };
    jitosol_ratio_cacheable_wrapper(resolver, req).await
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// IP to bind to.
    #[arg(long, env)]
    ip: IpAddr,

    /// Port to bind to.
    #[arg(long, env)]
    port: u16,

    /// Mongo connection URI.
    #[arg(long, env)]
    mongo_connection_uri: String,

    /// Mongo database name.
    #[arg(long, env)]
    mongo_db_name: String,

    /// Sentry API URL
    #[arg(long, env)]
    sentry_api_url: String,

    /// RPC URL
    #[arg(long, env)]
    rpc_url: String,

    /// Solana cluster e.g. testnet, mainnet, devnet
    #[arg(long, short, env, default_value_t=String::from("testnet"))]
    solana_cluster: String,
}

fn main() {
    let mut builder = Builder::new();
    builder
        .target(Target::Stdout)
        .filter_level(LevelFilter::Debug)
        .parse_default_env()
        .init();

    let args: Args = Args::parse();

    // Set up panic alerting via Sentry
    let _guard = sentry::init((
        args.sentry_api_url.clone(),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            ..Default::default()
        },
    ));
    info!("Sentry guard initialized");

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run_server(&args));
}

async fn run_server(args: &Args) {
    let c = Client::with_uri_str(args.mongo_connection_uri.clone())
        .await
        .expect("Mongo connection failed.");
    let db = c.database(&args.mongo_db_name);

    let query_resolver = QueryResolver::new(&db, &args.rpc_url);

    let cors = CorsLayer::new()
        .allow_headers(Any)
        .allow_methods(vec![Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any)
        .allow_credentials(false);

    let middleware = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(handle_error)) // handle middleware errors explicitly!
        .layer(BufferLayer::new(1000)) // backpressure when rate limits are hit
        .layer(RateLimitLayer::new(1000, Duration::from_secs(60)))
        .layer(TimeoutLayer::new(Duration::from_secs(10)))
        .layer(
            TraceLayer::new_for_http()
                .on_request(DefaultOnRequest::new().level(tracing_core::Level::INFO))
                .on_response(
                    DefaultOnResponse::new()
                        .level(tracing_core::Level::INFO)
                        .latency_unit(LatencyUnit::Micros),
                ),
        );

    let r = Router::new()
        .route(
            "/api/v1/stake_pool_stats",
            get(stake_pool_stats_handler).post(stake_pool_stats_handler),
        )
        .route(
            "/api/v1/jito_stake_over_time",
            get(jito_stake_over_time_handler).post(jito_stake_over_time_handler),
        )
        .route(
            "/api/v1/mev_commission_average_over_time",
            get(mev_commission_average_over_time_handler)
                .post(mev_commission_average_over_time_handler),
        )
        .route(
            "/api/v1/validators",
            get(validators_handler).post(validators_handler),
        )
        .route(
            "/api/v1/jitosol_validators",
            get(jitosol_validators_handler).post(jitosol_validators_handler),
        )
        .route(
            "/api/v1/validators/:vote_account",
            get(validator_by_vote_account_handler),
        )
        .route(
            "/api/v1/mev_rewards",
            get(mev_rewards_handler).post(mev_rewards_handler),
        )
        .route("/api/v1/daily_mev_rewards", get(daily_mev_rewards_handler))
        .route(
            "/api/v1/steward_events",
            get(steward_events_handler).post(steward_events_handler),
        )
        .route(
            "/api/v1/staker_rewards",
            get(staker_rewards_handler_v1).post(staker_rewards_handler_v1),
        )
        .route(
            "/api/v1/validator_rewards",
            get(validator_rewards_handler).post(validator_rewards_handler),
        )
        .route(
            "/api/v1/jitosol_sol_ratio",
            get(jitosol_sol_ratio_handler).post(jitosol_sol_ratio_handler),
        )
        .layer(Extension(query_resolver))
        .layer(middleware)
        .layer(cors);

    let addr = SocketAddr::new(args.ip, args.port);
    info!("Accepting requests at {addr}");

    Server::bind(&addr)
        .serve(r.into_make_service())
        .await
        .unwrap();
}
