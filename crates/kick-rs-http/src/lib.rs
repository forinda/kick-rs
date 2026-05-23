#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

pub mod bootstrap;
pub mod contributors;
#[cfg(feature = "devtools")]
pub mod devtools;
pub mod error;
pub mod http_plugin;
pub mod inject;
pub mod middleware;
pub mod module;
pub mod module_list;
#[cfg(feature = "openapi")]
pub mod openapi;
pub mod plugins;

pub use bootstrap::{bootstrap, AppState, Bootstrap};
pub use contributors::{contributors_middleware, Ctx};
pub use error::{HttpError, HttpResult};
pub use http_plugin::HttpPlugin;
pub use inject::Inject;
pub use middleware::{MiddlewareEntry, MiddlewarePhase};
pub use module::{define_module, HttpModule, HttpModuleBuilder};
pub use module_list::{define_modules, ModuleList, ModuleRegistry};

// Re-export the public surface of kick_rs_core so app code can write
// `use kick_rs_http::*;` without also pulling in `kick_rs_core`.
pub use kick_rs_core::{
    Adapter, AdapterContext, AdapterDef, AdapterFactory, BuildContext, Container, ContainerBuilder,
    ContextContributor, ContributorPipeline, ContributorRequest, ContributorRequestExt,
    ContributorStore, KickError, KickResult, Module, ModuleBuilder, Plugin, PluginDef,
    PluginFactory,
};
