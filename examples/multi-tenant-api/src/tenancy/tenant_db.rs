//! `TenantDb` value + `LoadTenantDb` contributor.

// The `ContributorRequest` and `KickResult` names are used in the
// `#[contributor]` signature below; the macro rewrites those types to
// absolute paths, so they appear unused at the import site even though
// the user-visible API requires them in scope. Mirrors the same wart
// in the kick-rs-macros integration tests.
#![allow(unused_imports)]

use super::{Tenant, TenantPoolRegistry};
use kick_rs::{contributor, ContributorRequest, ContributorRequestExt, KickError, KickResult};
use sqlx::PgPool;
use std::sync::Arc;

/// Per-request handle to the tenant's `PgPool`. Cheap to clone (`Arc`
/// internally). Handlers grab one as `Ctx<TenantDb>` and execute queries
/// against the tenant's schema with no further routing logic.
#[derive(Clone)]
pub struct TenantDb {
    /// Useful for logging / debug headers; not all handlers read it,
    /// but worth keeping accessible.
    pub tenant_slug: String,
    pool: Arc<PgPool>,
}

impl TenantDb {
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[contributor]
pub async fn LoadTenantDb(
    ctx: &dyn ContributorRequest,
    tenant: &Tenant,
) -> KickResult<TenantDb> {
    let registry = ctx.inject::<TenantPoolRegistry>();
    let pool = registry.pool_for(&tenant.slug).await.map_err(|e| {
        KickError::new(
            "RK_A_TENANT_DB",
            format!("could not acquire pool for tenant `{}`: {e}", tenant.slug),
        )
    })?;
    Ok(TenantDb {
        tenant_slug: tenant.slug.clone(),
        pool,
    })
}
