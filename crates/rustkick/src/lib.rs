//! # rustkick
//!
//! A Rust port of [KickJS](https://github.com/forinda/kick-js) — a
//! module-driven web framework on axum.
//!
//! This crate is an **umbrella**: it re-exports
//! [`rustkick-core`](rustkick_core) and [`rustkick-http`](rustkick_http)
//! so app code can write a single `use rustkick::*;`.
//!
//! See the workspace [`README.md`](../README.md) for the project tour,
//! [`SPEC.md`](../SPEC.md) for the design spec, and
//! [`ARCHITECTURE.md`](../ARCHITECTURE.md) for internals.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

// ── Core ────────────────────────────────────────────────────────────────
pub use rustkick_core::{
    define_adapter, define_module, define_plugin, Adapter, AdapterContext, ContextContributor,
    Container, ContainerBuilder, Introspect, IntrospectionKind, IntrospectionSnapshot, KickError,
    KickResult, Module, ModuleBuilder, Plugin, Scope, Token,
};

// ── HTTP ────────────────────────────────────────────────────────────────
pub use rustkick_http::{bootstrap, Bootstrap, Ctx, Inject, RequestContext};

// ── Optional crates re-exported under features ─────────────────────────
#[cfg(feature = "macros")]
pub use rustkick_macros as macros;

#[cfg(feature = "config")]
pub use rustkick_config as config;

#[cfg(feature = "assets")]
pub use rustkick_assets as assets;
