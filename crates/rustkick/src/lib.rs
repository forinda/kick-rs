//! # rustkick
//!
//! A Rust port of [KickJS](https://github.com/forinda/kick-js) — a
//! module-driven web framework on axum.
//!
//! This crate is an **umbrella**: it re-exports
//! [`rustkick-core`](rustkick_core) and [`rustkick-http`](rustkick_http)
//! so app code can write a single `use rustkick::*;`.
//!
//! When both crates expose a `define_module` symbol the HTTP one wins —
//! that's the one app authors want by default. The transport-agnostic
//! core variant remains reachable as [`rustkick_core::define_module`].
//!
//! See the workspace [`README.md`](../README.md) for the project tour,
//! [`SPEC.md`](../SPEC.md) for the design spec, and
//! [`ARCHITECTURE.md`](../ARCHITECTURE.md) for internals.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

// ── Core (transport-agnostic types) ────────────────────────────────────
pub use rustkick_core::{
    define_adapter, define_plugin, Adapter, AdapterContext, AdapterDef, AdapterFactory,
    BuildContext, ContextContributor, Container, ContainerBuilder, Introspect, IntrospectionKind,
    IntrospectionSnapshot, KickError, KickResult, Module as CoreModule,
    ModuleBuilder as CoreModuleBuilder, Plugin, PluginDef, PluginFactory, ProviderSpec, Scope,
    Token,
};

// ── HTTP (the default user-facing surface) ─────────────────────────────
pub use rustkick_http::{
    bootstrap, define_module, AppState, Bootstrap, Ctx, HttpError, HttpModule, HttpModuleBuilder,
    HttpResult, Inject, RequestContext,
};

// `Module` and `ModuleBuilder` in the umbrella refer to the HTTP variants
// since they're what app authors compose against. The core ones are
// available as `CoreModule` / `CoreModuleBuilder` above.
pub use rustkick_http::HttpModule as Module;
pub use rustkick_http::HttpModuleBuilder as ModuleBuilder;

// ── Optional crates re-exported under features ─────────────────────────
#[cfg(feature = "macros")]
pub use rustkick_macros as macros;

#[cfg(feature = "config")]
pub use rustkick_config as config;

#[cfg(feature = "assets")]
pub use rustkick_assets as assets;
