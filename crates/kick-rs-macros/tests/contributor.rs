//! Integration tests for `#[contributor]`. Mirrors the test structure
//! of `tests/service.rs`.

// The `ContributorRequest` import is used in the user-visible
// `#[contributor]` signatures below; the macro rewrites those to the
// absolute path, so clippy sees the import as unused even though the
// public API requires it. Keep the import for legibility + suppress.
#![allow(unused_imports)]

use kick_rs_core::{
    Container, ContributorPipeline, ContributorRequest, ContributorRequestExt, ContributorStore,
};
use kick_rs_macros::contributor;

// ── Domain types ─────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
struct Tenant {
    id: u32,
}

#[derive(Debug, PartialEq)]
struct Project {
    tenant_id: u32,
    name: &'static str,
}

#[derive(Debug, PartialEq)]
struct Cfg {
    seed: u32,
}

// ── Bare contributor (no deps, no ctx) ───────────────────────────────────

#[contributor]
async fn LoadTenant() -> kick_rs_core::KickResult<Tenant> {
    Ok(Tenant { id: 42 })
}

#[tokio::test]
async fn bare_contributor_runs() {
    let p = ContributorPipeline::build(vec![kick_rs_core::erase_contributor(LoadTenant)]).unwrap();
    let mut store = ContributorStore::new();
    p.run(&mut store).await.unwrap();
    assert_eq!(store.get::<Tenant>(), Some(&Tenant { id: 42 }));
}

// ── Single dep ───────────────────────────────────────────────────────────

#[contributor]
async fn LoadProject(tenant: &Tenant) -> kick_rs_core::KickResult<Project> {
    Ok(Project {
        tenant_id: tenant.id,
        name: "kick-rs",
    })
}

#[tokio::test]
async fn dep_arg_becomes_deps_tuple() {
    let p = ContributorPipeline::build(vec![
        kick_rs_core::erase_contributor(LoadTenant),
        kick_rs_core::erase_contributor(LoadProject),
    ])
    .unwrap();

    let mut store = ContributorStore::new();
    p.run(&mut store).await.unwrap();

    assert_eq!(
        store.get::<Project>(),
        Some(&Project {
            tenant_id: 42,
            name: "kick-rs"
        })
    );
}

// ── ctx + DI inject ──────────────────────────────────────────────────────

#[contributor]
async fn LoadFromConfig(ctx: &dyn ContributorRequest) -> kick_rs_core::KickResult<Tenant> {
    let cfg = ctx.inject::<Cfg>();
    Ok(Tenant { id: cfg.seed })
}

#[tokio::test]
async fn ctx_inject_via_container_works() {
    // Container provides Cfg; pipeline runs with the container attached.
    let container = Container::builder()
        .singleton(Cfg { seed: 7 })
        .build()
        .unwrap();

    let p =
        ContributorPipeline::build(vec![kick_rs_core::erase_contributor(LoadFromConfig)]).unwrap();
    let mut store = ContributorStore::with_container(container);
    p.run(&mut store).await.unwrap();

    assert_eq!(store.get::<Tenant>(), Some(&Tenant { id: 7 }));
}

// ── ctx + dep mixed ──────────────────────────────────────────────────────

#[contributor]
async fn LoadProjectViaInject(
    ctx: &dyn ContributorRequest,
    tenant: &Tenant,
) -> kick_rs_core::KickResult<Project> {
    // Pulls Cfg from DI to derive the project name, demonstrating
    // ctx + dep coexist in the same contributor signature.
    let cfg = ctx.inject::<Cfg>();
    Ok(Project {
        tenant_id: tenant.id,
        name: if cfg.seed > 0 { "kick-rs" } else { "fallback" },
    })
}

#[tokio::test]
async fn ctx_and_dep_coexist() {
    let container = Container::builder()
        .singleton(Cfg { seed: 1 })
        .build()
        .unwrap();
    let p = ContributorPipeline::build(vec![
        kick_rs_core::erase_contributor(LoadTenant),
        kick_rs_core::erase_contributor(LoadProjectViaInject),
    ])
    .unwrap();
    let mut store = ContributorStore::with_container(container);
    p.run(&mut store).await.unwrap();
    assert_eq!(
        store.get::<Project>(),
        Some(&Project {
            tenant_id: 42,
            name: "kick-rs"
        })
    );
}
