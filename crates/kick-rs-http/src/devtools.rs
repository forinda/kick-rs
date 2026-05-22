//! DevTools `/__debug` endpoint.
//!
//! Behind the `devtools` cargo feature. Enabling the feature *and*
//! calling [`crate::Bootstrap::with_devtools`] mounts a `GET /__debug`
//! route that returns a JSON snapshot of the assembled app:
//!
//! ```text
//! {
//!   "framework": "kick-rs",
//!   "version":   "0.1.0-alpha.1",
//!   "modules":   [ { "name": "users", "prefix": "/users", "routes": 5, "sub_modules": [] } ],
//!   "plugins":   [ { "name": "request-id" }, { "name": "openapi" } ],
//!   "adapters":  [ { "name": "...", "depends_on": [] } ],
//!   "contributors": { "count": 2 }
//! }
//! ```
//!
//! Two opt-ins (feature flag AND builder call) on purpose — defense
//! in depth against accidentally exposing this in production builds.

use crate::module::HttpModule;
use kick_rs_core::{Adapter, AnyContributor, Plugin};
use serde::Serialize;
use std::sync::Arc;

/// Default mount path. Override via [`crate::Bootstrap::with_devtools_at`].
pub const DEFAULT_PATH: &str = "/__debug";

/// Top-level snapshot. Serialized as the `/__debug` response body.
#[derive(Debug, Clone, Serialize)]
pub struct DebugSnapshot {
    /// Always `"kick-rs"`. Lets clients distinguish kick-rs snapshots
    /// from anything else they might be polling.
    pub framework: &'static str,
    /// Version of the `kick-rs-http` crate that built this snapshot.
    pub version: &'static str,
    /// User-mounted modules + any HTTP-plugin-contributed modules.
    pub modules: Vec<ModuleInfo>,
    /// Every registered plugin (core + HTTP).
    pub plugins: Vec<PluginInfo>,
    /// Every registered adapter, post-topo-sort.
    pub adapters: Vec<AdapterInfo>,
    /// Contributor pipeline summary.
    pub contributors: ContributorsInfo,
}

/// One module — name, prefix, route count, and recursive sub-modules.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleInfo {
    /// Stable module name (the argument to `define_module(...)`).
    pub name: String,
    /// URL prefix applied to every route on this module.
    pub prefix: String,
    /// Direct (non-recursive) route count.
    pub routes: usize,
    /// Nested sub-modules — each with its own routes + sub-modules.
    pub sub_modules: Vec<ModuleInfo>,
}

/// One plugin entry. Carries the name plus an optional free-form
/// state field surfaced by [`Plugin::introspect`](kick_rs_core::Plugin::introspect).
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    /// Plugin name from `Plugin::name()`.
    pub name: String,
    /// Component-specific state from `Plugin::introspect()`. `None`
    /// when the plugin doesn't opt in. Skipped from the JSON output
    /// when `None` so the response stays compact for the common case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<serde_json::Value>,
}

/// One adapter entry: name, declared `depends_on`, and optional
/// state from [`Adapter::introspect`](kick_rs_core::Adapter::introspect).
#[derive(Debug, Clone, Serialize)]
pub struct AdapterInfo {
    /// Adapter name from `Adapter::name()`.
    pub name: String,
    /// Other adapter names this one mounts after.
    pub depends_on: Vec<String>,
    /// Component-specific state from `Adapter::introspect()`. Same
    /// opt-in / skip-when-`None` rule as [`PluginInfo::state`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<serde_json::Value>,
}

/// Contributor pipeline summary. Keeping it to a count for now —
/// individual contributors don't have stable display names that would
/// be more useful than their TypeId.
#[derive(Debug, Clone, Serialize)]
pub struct ContributorsInfo {
    /// Total number of contributors registered across all sources
    /// (bootstrap-global, modules, plugins, adapters).
    pub count: usize,
}

fn module_info(m: &HttpModule) -> ModuleInfo {
    ModuleInfo {
        name: m.name().to_owned(),
        prefix: m.prefix().to_owned(),
        // Direct (non-recursive) route count. Sub-modules are nested in
        // sub_modules so adopters can see the tree shape.
        routes: m.routes.len(),
        sub_modules: m.sub_modules.iter().map(module_info).collect(),
    }
}

/// Build a snapshot from the bootstrap-time aggregated state.
pub(crate) fn build_snapshot(
    modules: &[HttpModule],
    plugins: &[Arc<dyn Plugin>],
    http_plugins: &[Arc<dyn crate::HttpPlugin>],
    adapters: &[Arc<dyn Adapter>],
    contributors: &[AnyContributor],
) -> DebugSnapshot {
    let modules = modules.iter().map(module_info).collect();
    let mut plugin_infos: Vec<PluginInfo> = plugins
        .iter()
        .map(|p| PluginInfo {
            name: p.name().to_owned(),
            state: p.introspect().map(|snap| snap.state),
        })
        .collect();
    plugin_infos.extend(http_plugins.iter().map(|p| PluginInfo {
        name: p.name().to_owned(),
        state: p.introspect().map(|snap| snap.state),
    }));
    let adapters = adapters
        .iter()
        .map(|a| AdapterInfo {
            name: a.name().to_owned(),
            depends_on: a.depends_on().iter().map(|s| s.to_string()).collect(),
            state: a.introspect().map(|snap| snap.state),
        })
        .collect();
    DebugSnapshot {
        framework: "kick-rs",
        version: env!("CARGO_PKG_VERSION"),
        modules,
        plugins: plugin_infos,
        adapters,
        contributors: ContributorsInfo {
            count: contributors.len(),
        },
    }
}

/// Build an `axum::Router` carrying a single `GET <path>` handler that
/// returns the snapshot as `application/json`. Called by
/// [`crate::Bootstrap::into_router`] when devtools is enabled.
pub(crate) fn snapshot_router(path: &str, snapshot: Arc<DebugSnapshot>) -> axum::Router {
    let body = Arc::new(
        serde_json::to_string(&*snapshot)
            .expect("DebugSnapshot must serialize — all fields are owned + serde-derived"),
    );
    axum::Router::new().route(
        path,
        axum::routing::get(move || {
            let body = body.clone();
            async move { ([("content-type", "application/json")], (*body).clone()) }
        }),
    )
}

#[cfg(test)]
mod tests {
    use crate::{bootstrap, define_module};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn root() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn debug_endpoint_returns_snapshot() {
        let m = define_module("greeter")
            .prefix("/g")
            .get("/", root)
            .get("/named/:n", root)
            .build();

        let (router, _) = bootstrap().module(m).with_devtools().into_router().unwrap();

        let res = router
            .oneshot(Request::get("/__debug").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json"
        );

        let body = axum::body::to_bytes(res.into_body(), 65536).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["framework"], "kick-rs");
        assert_eq!(parsed["modules"][0]["name"], "greeter");
        assert_eq!(parsed["modules"][0]["prefix"], "/g");
        assert_eq!(parsed["modules"][0]["routes"], 2);
        assert_eq!(parsed["contributors"]["count"], 0);
    }

    #[tokio::test]
    async fn debug_endpoint_off_by_default() {
        let m = define_module("g").get("/x", root).build();
        let (router, _) = bootstrap().module(m).into_router().unwrap();
        let res = router
            .oneshot(Request::get("/__debug").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "/__debug must not be mounted unless .with_devtools() was called"
        );
    }

    #[tokio::test]
    async fn debug_endpoint_supports_custom_path() {
        let m = define_module("g").get("/x", root).build();
        let (router, _) = bootstrap()
            .module(m)
            .with_devtools_at("/internal/state")
            .into_router()
            .unwrap();

        let res = router
            .oneshot(Request::get("/internal/state").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Default path is *not* mounted when a custom one was supplied.
        let (router2, _) = bootstrap()
            .module(define_module("g").get("/x", root).build())
            .with_devtools_at("/internal/state")
            .into_router()
            .unwrap();
        let res = router2
            .oneshot(Request::get("/__debug").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[cfg(feature = "plugin-request-id")]
    #[tokio::test]
    async fn snapshot_includes_introspect_state_for_opted_in_plugin() {
        use crate::plugins::request_id::RequestIdPlugin;

        let m = define_module("g").get("/x", root).build();
        let (router, _) = bootstrap()
            .module(m)
            .http_plugin(RequestIdPlugin)
            .with_devtools()
            .into_router()
            .unwrap();

        let res = router
            .oneshot(Request::get("/__debug").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 65536).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Find the request-id plugin in the snapshot and check its
        // state payload landed inline.
        let plugins = parsed["plugins"].as_array().expect("plugins is array");
        let entry = plugins
            .iter()
            .find(|p| p["name"] == "request-id")
            .expect("request-id plugin should appear in the snapshot");
        assert_eq!(entry["state"]["header"], "x-request-id");
        assert_eq!(entry["state"]["id_format"], "uuid-v7-or-passthrough");
    }

    #[tokio::test]
    async fn snapshot_skips_state_for_plugins_that_dont_opt_in() {
        // Build a tiny plugin that doesn't override introspect() —
        // the default impl returns None, so the JSON `state` field
        // should be omitted entirely (#[serde(skip_serializing_if)]).
        struct Silent;
        impl kick_rs_core::Plugin for Silent {
            fn name(&self) -> &str {
                "silent"
            }
        }

        let m = define_module("g").get("/x", root).build();
        let (router, _) = bootstrap()
            .module(m)
            .plugin(Silent)
            .with_devtools()
            .into_router()
            .unwrap();

        let res = router
            .oneshot(Request::get("/__debug").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), 65536).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let entry = parsed["plugins"]
            .as_array()
            .and_then(|a| a.iter().find(|p| p["name"] == "silent"))
            .expect("silent plugin should be in the snapshot");
        // skip_serializing_if = "Option::is_none" — no `state` key at all.
        assert!(entry.get("state").is_none(), "got: {entry}");
    }
}
