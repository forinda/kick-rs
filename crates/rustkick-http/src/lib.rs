//! # rustkick-http
//!
//! axum integration for rustkick. Provides:
//!
//! - [`bootstrap`] — the app entry-point builder
//! - [`Inject<T>`] — axum extractor backed by [`rustkick_core::Container`]
//! - [`RequestContext`] / [`Ctx`] — typed per-request context
//!
//! HTTP-specific built-in plugins (`request_id`, `request_logger`, …) live
//! in [`plugins`].

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod bootstrap;
pub mod context;
pub mod inject;
pub mod plugins;

pub use bootstrap::{bootstrap, Bootstrap};
pub use context::{Ctx, RequestContext};
pub use inject::Inject;
