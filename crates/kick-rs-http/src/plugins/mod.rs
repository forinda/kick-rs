//! Built-in `HttpPlugin`s shipped with `kick-rs-http`.
//!
//! Each plugin is feature-gated so adopters can opt out:
//!
//! ```toml
//! # take everything (default)
//! kick-rs-http = "0.1.0-alpha.1"
//!
//! # opt out of one
//! kick-rs-http = { version = "0.1.0-alpha.1", default-features = false,
//!                 features = ["plugin-request-id", "plugin-cors"] }
//! ```
//!
//! Recommended composition (request-flow order):
//!
//! ```ignore
//! bootstrap()
//!     .http_plugin(plugins::cors::CorsPlugin::permissive())
//!     .http_plugin(plugins::request_id::RequestIdPlugin::default())
//!     .http_plugin(plugins::request_logger::RequestLoggerPlugin::default())
//!     .http_plugin(plugins::compression::CompressionPlugin::default())
//!     .module(users_module())
//!     .listen(addr).await
//! ```

#[cfg(feature = "plugin-cors")]
pub mod cors;

#[cfg(feature = "plugin-compression")]
pub mod compression;

#[cfg(feature = "plugin-request-id")]
pub mod request_id;

#[cfg(feature = "plugin-request-logger")]
pub mod request_logger;

#[cfg(feature = "plugin-helmet")]
pub mod helmet;

#[cfg(feature = "plugin-trace-context")]
pub mod trace_context;

#[cfg(feature = "plugin-assets")]
pub mod assets;
