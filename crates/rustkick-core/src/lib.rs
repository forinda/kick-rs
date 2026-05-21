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

pub mod container;
pub mod error;
pub mod token;
pub mod scope;
pub mod module;
pub mod adapter;
pub mod plugin;
pub mod contributor;
pub mod mount_sort;
pub mod introspect;

pub use container::{Container, ContainerBuilder};
pub use error::{KickError, KickResult};
pub use token::Token;
pub use scope::Scope;
pub use module::{define_module, Module, ModuleBuilder, ProviderSpec};
pub use adapter::{define_adapter, Adapter, AdapterContext, AdapterDef, AdapterFactory, BuildContext};
pub use plugin::{define_plugin, Plugin, PluginDef, PluginFactory};
pub use contributor::ContextContributor;
pub use introspect::{Introspect, IntrospectionKind, IntrospectionSnapshot};
