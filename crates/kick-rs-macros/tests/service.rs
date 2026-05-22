//! Integration tests for `#[service]`. Lives in `tests/` so the crate
//! can use its own proc-macros (which isn't allowed in unit tests of a
//! `proc-macro = true` crate).

use kick_rs_core::{Container, ServiceImpl};
use kick_rs_macros::service;
use std::sync::Arc;

// ── A simple Arc<T> field ─────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
struct Db {
    url: String,
}

#[service]
struct Repo {
    db: Arc<Db>,
}

#[test]
fn arc_field_resolves_from_container() {
    let c = Container::builder()
        .singleton(Db {
            url: "postgres://localhost".into(),
        })
        .build()
        .unwrap();

    let repo: Arc<Repo> = Repo::build(&c);
    assert_eq!(repo.db.url, "postgres://localhost");
}

// ── Multiple Arc<T> fields ────────────────────────────────────────────────

struct Cache {
    name: &'static str,
}

#[service]
struct Aggregator {
    db: Arc<Db>,
    cache: Arc<Cache>,
}

#[test]
fn multiple_arc_fields_all_resolve() {
    let c = Container::builder()
        .singleton(Db { url: "u".into() })
        .singleton(Cache { name: "primary" })
        .build()
        .unwrap();

    let a: Arc<Aggregator> = Aggregator::build(&c);
    assert_eq!(a.db.url, "u");
    assert_eq!(a.cache.name, "primary");
}

// ── Unit struct ───────────────────────────────────────────────────────────

#[service]
struct Marker;

#[test]
fn unit_struct_supported() {
    let c = Container::builder().build().unwrap();
    let m: Arc<Marker> = Marker::build(&c);
    // Smoke — Arc must wrap a usable value.
    assert!(Arc::strong_count(&m) >= 1);
}

// ── Inject<T> field is rewritten to Arc<T> ────────────────────────────────
//
// In real apps `Inject` is the axum extractor from `kick-rs-http`. The
// macro is happy with any path whose last segment is named `Inject` or
// `Arc` — what matters is that it returns the inner generic. Define a
// trivial `Inject` alias here so this test can run without pulling in
// the HTTP crate.

#[allow(dead_code)]
type Inject<T> = Arc<T>;

#[service]
struct WithInject {
    db: Inject<Db>,
}

#[test]
fn inject_field_is_rewritten_to_arc() {
    let c = Container::builder()
        .singleton(Db {
            url: "via-inject".into(),
        })
        .build()
        .unwrap();

    let w: Arc<WithInject> = WithInject::build(&c);
    assert_eq!(w.db.url, "via-inject");
}

// ── End-to-end: register via `.service::<T>()` ────────────────────────────

#[test]
fn module_service_method_uses_service_impl() {
    use kick_rs_core::define_module;

    let m = define_module("svc")
        .service_value(Db { url: "x".into() })
        .service::<Repo>()
        .build();

    let c = m.register_into(Container::builder()).build().unwrap();
    let repo = c.resolve::<Repo>();
    assert_eq!(repo.db.url, "x");
}
