//! `multi-tenant-api` — kick-rs example showing the tenant-DB factory
//! pattern via context contributors + a per-tenant pool registry.
//!
//! Run:
//! ```text
//! cp .env.example .env
//! docker compose up -d
//! cargo run
//! ```
//!
//! Then exercise it:
//! ```text
//! curl -H 'X-Tenant-Slug: acme'    http://localhost:3001/posts
//! curl -X POST -H 'X-Tenant-Slug: acme'    -H 'Content-Type: application/json' \
//!     -d '{"title":"hello","body":"from acme"}' http://localhost:3001/posts
//!
//! curl -H 'X-Tenant-Slug: globex' http://localhost:3001/posts
//! # ↑ returns [] — acme's post is isolated to acme's schema
//! ```

mod config;
mod modules;
mod tenancy;

use config::Env;
use kick_rs::{bootstrap, define_module, KickError, KickResult, Module};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tenancy::{LoadTenant, LoadTenantDb, TenantPoolRegistry, TenantsAllowlist};

#[tokio::main]
async fn main() -> KickResult<()> {
    install_tracing();

    let env = Env::load()?;
    tracing::info!(?env, "loaded config");

    // Bootstrap pool — only used to run migrations. The per-tenant
    // pools (created lazily by `TenantPoolRegistry`) handle the actual
    // request traffic.
    let migration_pool = open_pool(&env.database_url).await?;
    run_migrations(&migration_pool).await?;

    let registry = TenantPoolRegistry::new(env.database_url.clone(), /* max_conns */ 10);
    let allowlist = TenantsAllowlist { slugs: env.tenants.clone() };

    bootstrap()
        .module(infra_module(registry, allowlist))
        .module(modules::posts::define())
        .contribute(LoadTenant)
        .contribute(LoadTenantDb)
        .listen(&env.bind_addr)
        .await
}

/// Module that does nothing but expose two DI singletons:
/// - `TenantPoolRegistry` — used by `LoadTenantDb`
/// - `TenantsAllowlist`   — used by `LoadTenant`
fn infra_module(registry: TenantPoolRegistry, allowlist: TenantsAllowlist) -> Module {
    define_module("infra")
        .service_value(registry)
        .service_value(allowlist)
        .build()
}

async fn open_pool(url: &str) -> KickResult<PgPool> {
    PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(url)
        .await
        .map_err(|e| {
            KickError::new("RK_A_PG_CONNECT", format!("could not connect to postgres: {e}"))
                .with_hint("check MT_DATABASE_URL and that the server is up (`docker compose ps`)")
                .with_source(e)
        })
}

async fn run_migrations(pool: &PgPool) -> KickResult<()> {
    sqlx::migrate!("./migrations").run(pool).await.map_err(|e| {
        KickError::new("RK_A_MIGRATE", format!("migration run failed: {e}"))
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
