//! # rustkick-core
//!
//! Core building blocks for the rustkick framework — DI container, module
//! system, adapter/plugin factories, context contributors, error model.
//!
//! This crate has **no dependency on axum or any HTTP runtime** so it can be
//! reused for non-HTTP transports (workers, CLIs, future WS-only apps).
//!
//! See the workspace [`SPEC.md`](../SPEC.md) for the design overview and
//! [`ARCHITECTURE.md`](../ARCHITECTURE.md) for internals.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod adapter;
pub mod container;
pub mod contributor;
pub mod error;
pub mod introspect;
pub mod module;
pub mod mount_sort;
pub mod plugin;
pub mod scope;
pub mod token;

pub use adapter::{
    define_adapter, Adapter, AdapterContext, AdapterDef, AdapterFactory, BuildContext,
};
pub use container::{Container, ContainerBuilder};
pub use contributor::ContextContributor;
pub use error::{KickError, KickResult};
pub use introspect::{Introspect, IntrospectionKind, IntrospectionSnapshot};
pub use module::{define_module, Module, ModuleBuilder, ProviderSpec};
pub use plugin::{define_plugin, Plugin, PluginDef, PluginFactory};
pub use scope::Scope;
pub use token::Token;
