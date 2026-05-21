//! # rustkick-http
//!
//! axum integration for rustkick. Provides:
//!
//! - [`bootstrap`] — the app entry-point builder
//! - [`Inject<T>`] — axum extractor backed by [`rustkick_core::Container`]
//! - [`define_module`] / [`HttpModule`] — HTTP-aware module wrapper around
//!   [`rustkick_core::Module`] (adds routes + axum integration)
//! - [`HttpError`] / [`HttpResult`] — RFC 7807 problem-details response
//!   wrapper for [`rustkick_core::KickError`]
//!
//! HTTP-specific built-in plugins (`request_id`, `request_logger`, …) live
//! in [`plugins`].

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod bootstrap;
pub mod context;
pub mod error;
pub mod inject;
pub mod module;
pub mod plugins;

pub use bootstrap::{bootstrap, AppState, Bootstrap};
pub use context::{Ctx, RequestContext};
pub use error::{HttpError, HttpResult};
pub use inject::Inject;
pub use module::{define_module, HttpModule, HttpModuleBuilder};

// Re-export the public surface of rustkick_core so app code can write
// `use rustkick_http::*;` without also pulling in `rustkick_core`.
pub use rustkick_core::{
    Adapter, AdapterContext, AdapterDef, AdapterFactory, BuildContext, Container, ContainerBuilder,
    KickError, KickResult, Module, ModuleBuilder, Plugin, PluginDef, PluginFactory,
};
