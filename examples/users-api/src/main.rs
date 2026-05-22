//! `users-api` — minimal kick-rs example.
//!
//! Wiring:
//! 1. Load config via `kick_rs::config::Config` (defaults + `.env` + `USERS_*` env)
//! 2. Open a sqlx PgPool
//! 3. Run pending up-migrations via `sqlx::migrate!()` (embeds the
//!    `migrations/` directory into the binary at compile time)
//! 4. Hand the pool to kick-rs as a DI singleton via a small infra
//!    module
//! 5. Mount the `users` module + an `OpenApiPlugin` that serves
//!    `/openapi.json` (auto-collected from the module's `paths!(...)`
//!    registrations) and listen
//!
//! Run:
//! ```text
//! cp .env.example .env
//! docker compose up -d
//! cargo run
//! ```

mod config;
mod modules;

use config::Env;
use kick_rs::openapi::OpenApiPlugin;
use kick_rs::{bootstrap, define_module, KickError, KickResult, Module};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use utoipa::openapi::InfoBuilder;

#[tokio::main]
async fn main() -> KickResult<()> {
    install_tracing();

    let env = Env::load()?;
    tracing::info!(?env, "loaded config");

    let pool = open_pool(&env.database_url).await?;
    run_migrations(&pool).await?;

    // The users module owns its own utoipa paths — `OpenApiPlugin`
    // walks them and serves /openapi.json. Build the module first so
    // we can reference it before moving it into bootstrap().
    let users = modules::users::define();
    let openapi_plugin = OpenApiPlugin::from_modules(
        InfoBuilder::new()
            .title("users-api")
            .version(env!("CARGO_PKG_VERSION"))
            .build(),
        [&users],
    );

    bootstrap()
        .module(infra_module(pool))
        .http_plugin(openapi_plugin)
        .module(users)
        .listen(&env.bind_addr)
        .await
}

/// Build a one-provider module whose sole job is to expose the `PgPool`
/// as a singleton to the DI graph. Kept separate so the `users` module
/// stays focused on user-domain concerns.
///
/// We register the bare `PgPool` rather than `Arc<PgPool>` because the
/// container wraps the value in an `Arc<T>` internally, and consumers
/// of DI do `c.resolve::<PgPool>()` to get back an `Arc<PgPool>`.
fn infra_module(pool: PgPool) -> Module {
    define_module("infra").service_value(pool).build()
}

async fn open_pool(url: &str) -> KickResult<PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await
        .map_err(|e| {
            KickError::new("RK_A_PG_CONNECT", format!("could not connect to postgres: {e}"))
                .with_hint("check USERS_DATABASE_URL and that the server is up (`docker compose ps`)")
                .with_source(e)
        })
}

async fn run_migrations(pool: &PgPool) -> KickResult<()> {
    sqlx::migrate!("./migrations").run(pool).await.map_err(|e| {
        KickError::new("RK_A_MIGRATE", format!("migration run failed: {e}"))
            .with_hint("inspect `sqlx::migrate!()` output and roll back via `sqlx migrate revert`")
            .with_source(e)
    })?;
    tracing::info!("migrations up-to-date");
    Ok(())
}

fn install_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).with_target(true).init();
}
