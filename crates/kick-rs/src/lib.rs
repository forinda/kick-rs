#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

// ── Core (transport-agnostic types) ────────────────────────────────────
pub use kick_rs_core::{
    define_adapter, define_plugin, Adapter, AdapterContext, AdapterDef, AdapterFactory,
    BuildContext, Container, ContainerBuilder, ContextContributor, ContributorDeps,
    ContributorPipeline, ContributorRequest, ContributorRequestExt, ContributorStore, Introspect,
    IntrospectionKind, IntrospectionSnapshot, KickError, KickResult, Module as CoreModule,
    ModuleBuilder as CoreModuleBuilder, OnErrorAction, Plugin, PluginDef, PluginFactory,
    ProviderSpec, Scope, ServiceImpl, Token,
};

// ── HTTP (the default user-facing surface) ─────────────────────────────
pub use kick_rs_http::{
    bootstrap, contributors_middleware, define_module, define_modules, AppState, Bootstrap, Ctx,
    HttpError, HttpModule, HttpModuleBuilder, HttpPlugin, HttpResult, Inject, MiddlewareEntry,
    MiddlewarePhase, ModuleList, ModuleRegistry,
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
//   kick-rs = { version = "0.1", features = ["macros"] }
//
// The `assets` re-export will return once `kick-rs-assets` reaches
// publish-ready state. Adopters who need it earlier can depend on it
// directly.
#[cfg(feature = "macros")]
pub use kick_rs_macros::*;

/// Internal re-export used by macros from `kick-rs-macros` (the
/// `paths!` macro emits references to `kick_rs::__http::openapi::*`
/// when the umbrella is the resolved dependency, since adopters using
/// the umbrella don't list `kick-rs-http` directly).
#[doc(hidden)]
pub use kick_rs_http as __http;

// ── Opt-in OpenAPI integration (feature = "openapi") ───────────────────
//
// Surface `kick_rs_http::openapi` cleanly so adopters write
// `kick_rs::openapi::OpenApiPlugin::from_modules(...)` rather than
// reaching into the doc-hidden umbrella alias.
#[cfg(feature = "openapi")]
pub use kick_rs_http::openapi;

// ── Opt-in DevTools introspection (feature = "devtools") ───────────────
//
// Exposes the `devtools` module so adopters can name the snapshot
// types in their own code (e.g. to consume the JSON in tests).
#[cfg(feature = "devtools")]
pub use kick_rs_http::devtools;

// ── Opt-in config loader (feature = "config") ──────────────────────────
//
// Layered env / dotenv / TOML / JSON loader. Exposes the entire
// `kick-rs-config` surface under `kick_rs::config`.
#[cfg(feature = "config")]
pub use kick_rs_config as config;

// ── Opt-in static-assets (feature = "assets") ──────────────────────────
//
// Bundling (compile time) + serving (runtime), under one path so
// adopters write a single `use kick_rs::assets::{...};`.
#[cfg(feature = "assets")]
pub mod assets {
    //! Static-asset bundling + HTTP serving.
    //!
    //! Compile-time bundling primitives come from `kick-rs-assets`:
    //! the [`embed_assets!`](kick_rs_assets::embed_assets) macro,
    //! the [`EmbeddedAssets`](kick_rs_assets::EmbeddedAssets) tree
    //! it returns, [`AssetManifest`](kick_rs_assets::AssetManifest)
    //! for `key → hashed-name` lookup, and
    //! [`content_type_for`](kick_rs_assets::content_type_for) for
    //! MIME inference.
    //!
    //! HTTP-side: [`AssetsPlugin`] takes the manifest + the
    //! embedded tree and serves them under the manifest's URL
    //! prefix with `cache-control: public, immutable, max-age=…`.

    pub use kick_rs_assets::*;
    pub use kick_rs_http::plugins::assets::AssetsPlugin;
}
