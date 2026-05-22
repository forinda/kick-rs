//! `Tenant` value + `LoadTenant` contributor.

// The `ContributorRequest` and `KickResult` imports are used inside the
// `#[contributor]` signature; the macro rewrites them to absolute paths
// so they appear unused at the import site. Same wart as in
// `tenant_db.rs`.
#![allow(unused_imports)]

use axum::http::HeaderMap;
use kick_rs::{contributor, ContributorRequest, ContributorRequestExt, KickError, KickResult};

/// Identifies which tenant this request belongs to. Slug is the
/// canonical form used in URLs, schema names, headers, etc.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Tenant {
    pub slug: String,
}

/// Allowlist of acceptable tenant slugs, bound as a singleton at
/// startup. `LoadTenant` rejects requests for any tenant not in this
/// list with a structured error.
#[derive(Debug, Clone)]
pub struct TenantsAllowlist {
    pub slugs: Vec<String>,
}

impl TenantsAllowlist {
    pub fn contains(&self, slug: &str) -> bool {
        self.slugs.iter().any(|s| s == slug)
    }
}

const HEADER: &str = "x-tenant-slug";

#[contributor]
pub async fn LoadTenant(
    ctx: &dyn ContributorRequest,
    headers: &HeaderMap,
) -> KickResult<Tenant> {
    let raw = headers
        .get(HEADER)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            KickError::new(
                "RK_A_TENANT_MISSING",
                format!("required header `{HEADER}` is missing"),
            )
            .with_hint(format!("send `{HEADER}: <slug>` with every request"))
        })?
        .to_owned();

    // Allowlist check — DI singleton populated at bootstrap.
    let allowlist = ctx.inject::<TenantsAllowlist>();
    if !allowlist.contains(&raw) {
        return Err(KickError::new(
            "RK_A_TENANT_UNKNOWN",
            format!("`{raw}` is not a known tenant"),
        )
        .with_hint(format!(
            "known tenants: {}",
            allowlist.slugs.join(", ")
        )));
    }

    Ok(Tenant { slug: raw })
}
