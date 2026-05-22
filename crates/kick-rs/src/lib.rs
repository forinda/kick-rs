//! # kick-rs
//!
//! A Rust port of [KickJS](https://github.com/forinda/kick-js) — a
//! module-driven web framework on axum.
//!
//! This crate is an **umbrella**: it re-exports
//! [`kick-rs-core`](kick_rs_core) and [`kick-rs-http`](kick_rs_http)
//! so app code can write a single `use kick_rs::*;`.
//!
//! When both crates expose a `define_module` symbol the HTTP one wins —
//! that's the one app authors want by default. The transport-agnostic
//! core variant remains reachable as [`kick_rs_core::define_module`].
//!
//! See the workspace [`README.md`](../README.md) for the project tour,
//! [`SPEC.md`](../SPEC.md) for the design spec, and
//! [`ARCHITECTURE.md`](../ARCHITECTURE.md) for internals.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

// ── Core (transport-agnostic types) ────────────────────────────────────
pub use kick_rs_core::{
    define_adapter, define_plugin, Adapter, AdapterContext, AdapterDef, AdapterFactory,
    BuildContext, Container, ContainerBuilder, ContextContributor, ContributorDeps,
    ContributorPipeline, ContributorRequest, ContributorRequestExt, ContributorStore, Introspect,
    IntrospectionKind, IntrospectionSnapshot, KickError, KickResult, Module as CoreModule,
    ModuleBuilder as CoreModuleBuilder, Plugin, PluginDef, PluginFactory, ProviderSpec, Scope,
    ServiceImpl, Token,
};

// ── HTTP (the default user-facing surface) ─────────────────────────────
pub use kick_rs_http::{
    bootstrap, contributors_middleware, define_module, AppState, Bootstrap, Ctx, HttpError,
    HttpModule, HttpModuleBuilder, HttpPlugin, HttpResult, Inject,
};

// `Module` and `ModuleBuilder` in the umbrella refer to the HTTP variants
// since they're what app authors compose against. The core ones are
// available as `CoreModule` / `CoreModuleBuilder` above.
pub use kick_rs_http::HttpModule as Module;
pub use kick_rs_http::HttpModuleBuilder as ModuleBuilder;

// ── Opt-in proc-macros (feature = "macros") ────────────────────────────
//
// Re-exports `#[service]` (and any future proc-macros) from
// `kick-rs-macros`. Adopters opt in via:
//   kick-rs = { version = "0.0", features = ["macros"] }
//
// The `config` and `assets` re-exports will return once those crates
// reach publish-ready state (Phase 5). Adopters who need them earlier
// can depend on them directly via git.
#[cfg(feature = "macros")]
pub use kick_rs_macros::*;
