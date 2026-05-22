//! `TenantPoolRegistry` — singleton factory of per-tenant `PgPool`s.
//!
//! Each tenant gets its own `PgPool`, lazily created on first request.
//! Each pool is configured with `search_path = tenant_<slug>,public`
//! via `PgConnectOptions::options`, so SQL in the handler can write
//! `SELECT * FROM posts` and Postgres resolves it to the tenant's
//! schema automatically.
//!
//! Caching strategy: read-locked HashMap lookup on the hot path;
//! write lock only when a pool needs to be created. Double-checked
//! locking guards against two threads racing on first request.

use kick_rs::{KickError, KickResult};
use sqlx::postgres::PgPoolOptions;
use sqlx::{ConnectOptions, PgPool};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

/// Bound as a DI singleton. Cloning is cheap (Arc inside).
#[derive(Clone)]
pub struct TenantPoolRegistry {
    inner: Arc<RegistryInner>,
}

struct RegistryInner {
    base_url: String,
    max_conns_per_tenant: u32,
    pools: RwLock<HashMap<String, Arc<PgPool>>>,
}

impl TenantPoolRegistry {
    pub fn new(base_url: impl Into<String>, max_conns_per_tenant: u32) -> Self {
        Self {
            inner: Arc::new(RegistryInner {
                base_url: base_url.into(),
                max_conns_per_tenant,
                pools: RwLock::new(HashMap::new()),
            }),
        }
    }

    /// Return the `PgPool` for `slug`, creating it on first call.
    /// Pools are cached for the process lifetime.
    pub async fn pool_for(&self, slug: &str) -> KickResult<Arc<PgPool>> {
        // Fast path: read lock + cache hit.
        if let Some(pool) = self
            .inner
            .pools
            .read()
            .ok()
            .and_then(|m| m.get(slug).cloned())
        {
            return Ok(pool);
        }

        // Cache miss: build a pool. Do this BEFORE taking the write
        // lock so we don't hold the lock across an `await`.
        let pool = build_tenant_pool(
            &self.inner.base_url,
            slug,
            self.inner.max_conns_per_tenant,
        )
        .await?;
        let pool = Arc::new(pool);

        // Insert under write lock; if another thread won the race,
        // discard our pool and use theirs.
        let mut w = self.inner.pools.write().map_err(|_| poisoned())?;
        let pool = w.entry(slug.to_owned()).or_insert_with(|| pool).clone();
        Ok(pool)
    }
}

async fn build_tenant_pool(
    base_url: &str,
    slug: &str,
    max_conns: u32,
) -> KickResult<PgPool> {
    // Slugs are checked against the allowlist before they reach us, so
    // the schema name here is safe — but we still scrub the input to
    // belt-and-suspenders against future code paths bypassing
    // `LoadTenant`.
    let schema = format!("tenant_{}", scrub_slug(slug));

    let opts =
        sqlx::postgres::PgConnectOptions::from_str(base_url)
            .map_err(|e| {
                KickError::new("RK_A_PG_URL", format!("bad DATABASE_URL: {e}"))
                    .with_source(e)
            })?
            .options([("search_path", format!("{schema},public").as_str())]);

    PgPoolOptions::new()
        .max_connections(max_conns)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(opts)
        .await
        .map_err(|e| {
            KickError::new(
                "RK_A_PG_CONNECT",
                format!("could not connect to postgres for tenant `{slug}`: {e}"),
            )
            .with_source(e)
        })
}

fn scrub_slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn poisoned() -> KickError {
    KickError::new(
        "RK_A_POISONED",
        "TenantPoolRegistry lock poisoned — a prior thread panicked",
    )
}
