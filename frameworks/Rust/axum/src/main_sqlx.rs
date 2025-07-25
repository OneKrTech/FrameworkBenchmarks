mod common;
mod sqlx;

use std::{borrow::Cow, sync::Arc};

use ::sqlx::PgPool;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use dotenv::dotenv;
use quick_cache::sync::Cache;
use rand::{rngs::SmallRng, rng, SeedableRng};
use sqlx::models::World;
use yarte::Template;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[cfg(not(feature = "simd-json"))]
use axum::Json;
#[cfg(feature = "simd-json")]
use common::simd_json::Json;

mod server;

use common::{
    get_env, random_id, random_ids,
    utils::{parse_params, Params, Utf8Html},
};
use sqlx::database::create_pool;
use sqlx::models::Fortune;

#[derive(Template)]
#[template(path = "fortunes.html.hbs")]
pub struct FortunesTemplate<'a> {
    pub fortunes: &'a Vec<Fortune>,
}

async fn db(State(AppState { db, .. }): State<AppState>) -> impl IntoResponse {
    let id = random_id(&mut rng());
    let world: World = ::sqlx::query_as(common::SELECT_WORLD_BY_ID)
        .bind(id)
        .fetch_one(&mut *db.acquire().await.unwrap())
        .await
        .expect("error loading world");

    (StatusCode::OK, Json(world))
}

async fn queries(
    State(AppState { db, .. }): State<AppState>,
    Query(params): Query<Params>,
) -> impl IntoResponse {
    let mut rng = SmallRng::from_rng(&mut rng());
    let count = parse_params(params);
    let mut worlds: Vec<World> = Vec::with_capacity(count);

    for id in random_ids(&mut rng, count) {
        let world: World = ::sqlx::query_as(common::SELECT_WORLD_BY_ID)
            .bind(id)
            .fetch_one(&mut *db.acquire().await.unwrap())
            .await
            .expect("error loading world");
        worlds.push(world);
    }

    (StatusCode::OK, Json(worlds))
}

async fn fortunes(State(AppState { db, .. }): State<AppState>) -> impl IntoResponse {
    let mut fortunes: Vec<Fortune> = ::sqlx::query_as(common::SELECT_ALL_FORTUNES)
        .fetch_all(&mut *db.acquire().await.unwrap())
        .await
        .expect("error loading Fortunes");

    fortunes.push(Fortune {
        id: 0,
        message: Cow::Borrowed("Additional fortune added at request time."),
    });

    fortunes.sort_by(|a, b| a.message.cmp(&b.message));

    Utf8Html(
        FortunesTemplate {
            fortunes: &fortunes,
        }
        .call()
        .expect("error rendering template"),
    )
}

async fn cache(
    State(AppState { cache, .. }): State<AppState>,
    Query(params): Query<Params>,
) -> impl IntoResponse {
    let count = parse_params(params);
    let mut rng = SmallRng::from_rng(&mut rng());
    let mut worlds: Vec<Option<World>> = Vec::with_capacity(count);
    
    for id in random_ids(&mut rng, count) {
        worlds.push(cache.get(&id));
    }

    (StatusCode::OK, Json(worlds))
}

/// Pre-load the cache with all worlds.
async fn preload_cache(AppState { db, cache }: &AppState) {
    let worlds: Vec<World> = ::sqlx::query_as(common::SELECT_ALL_CACHED_WORLDS)
        .fetch_all(&mut *db.acquire().await.unwrap())
        .await
        .expect("error loading worlds");

    for world in worlds {
        cache.insert(world.id, world);
    }
}

/// Application state
#[derive(Clone)]
struct AppState {
    db: PgPool,
    cache: Arc<Cache<i32, World>>,
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let database_url: String = get_env("POSTGRES_URL");
    let max_pool_size: u32 = get_env("POSTGRES_MAX_POOL_SIZE");
    let min_pool_size: u32 = get_env("POSTGRES_MIN_POOL_SIZE");

    let state = AppState {
        db: create_pool(database_url, max_pool_size, min_pool_size).await,
        cache: Arc::new(Cache::new(10_000))
    };

    // Prime the cache with CachedWorld objects
    preload_cache(&state).await;

    let app = Router::new()
        .route("/fortunes", get(fortunes))
        .route("/db", get(db))
        .route("/queries", get(queries))
        .route("/cached-queries", get(cache))
        .with_state(state);

    server::serve_hyper(app, Some(8000)).await
}
