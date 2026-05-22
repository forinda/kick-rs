//! OpenAPI integration via [`utoipa`].
//!
//! Behind the `openapi` cargo feature. Adopters assemble a
//! `utoipa::openapi::OpenApi` themselves (typically via
//! `#[derive(utoipa::OpenApi)]`) and hand it to [`OpenApiPlugin::new`];
//! the plugin serves the spec at a configurable path (default
//! `/openapi.json`).
//!
//! This is deliberately *low-coupling*: we don't try to scrape the
//! `kick-rs` route macros into a utoipa schema for you. Use utoipa's
//! own `#[utoipa::path(...)]` attribute on your handlers alongside our
//! `#[get]/#[post]/...` macros — they don't fight each other.
//!
//! ```ignore
//! use utoipa::OpenApi;
//! use kick_rs_http::{bootstrap, openapi::OpenApiPlugin};
//!
//! #[derive(OpenApi)]
//! #[openapi(paths(crate::users::list, crate::users::get))]
//! struct ApiDoc;
//!
//! bootstrap()
//!     .http_plugin(OpenApiPlugin::new(ApiDoc::openapi()))
//!     .module(users_module())
//!     .listen("0.0.0.0:3000").await
//! ```

use crate::module::{define_module, HttpModule};
use kick_rs_core::Plugin;
use std::sync::Arc;
use utoipa::openapi::OpenApi;

const DEFAULT_PATH: &str = "/openapi.json";

/// Built-in plugin that serves a pre-rendered OpenAPI spec.
///
/// The spec is serialized to JSON *once* at construction time and
/// shared across requests via an [`Arc`], so per-request cost is just
/// a refcount bump and a clone of the cached JSON bytes — no
/// re-serialization on the hot path.
#[derive(Debug, Clone)]
pub struct OpenApiPlugin {
    json: Arc<String>,
    path: String,
}

impl OpenApiPlugin {
    /// Build the plugin from an assembled `OpenApi` value. The spec is
    /// serialized to JSON immediately; this panics if serialization
    /// fails, which in practice only happens if the `OpenApi` contains
    /// a [`serde_json::Map`] key that isn't a string — a programmer
    /// error in the upstream `#[derive(OpenApi)]`, not a runtime
    /// condition worth bubbling.
    pub fn new(spec: OpenApi) -> Self {
        let json = serde_json::to_string(&spec)
            .expect("OpenApi serialization should never fail for a valid utoipa spec");
        Self {
            json: Arc::new(json),
            path: DEFAULT_PATH.to_owned(),
        }
    }

    /// Override the path at which the spec is served. Default
    /// `/openapi.json`.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// The configured path. Useful for tests / logging.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl Plugin for OpenApiPlugin {
    fn name(&self) -> &str {
        "openapi"
    }
}

impl crate::HttpPlugin for OpenApiPlugin {
    fn http_modules(&self) -> Vec<HttpModule> {
        let json = self.json.clone();
        let handler = move || {
            let body = json.clone();
            async move { ([("content-type", "application/json")], (*body).clone()) }
        };
        vec![define_module("openapi").get(&self.path, handler).build()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap;
    use axum::body::Body;
    use axum::http::{Request as HReq, StatusCode};
    use tower::ServiceExt;
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(info(title = "kick-rs test api", version = "0.0.1"))]
    struct ApiDoc;

    #[tokio::test]
    async fn serves_openapi_json_at_default_path() {
        let (router, _) = bootstrap()
            .http_plugin(OpenApiPlugin::new(ApiDoc::openapi()))
            .into_router()
            .unwrap();

        let res = router
            .oneshot(HReq::get("/openapi.json").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "application/json"
        );
        let body = axum::body::to_bytes(res.into_body(), 65536).await.unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(parsed["info"]["title"], "kick-rs test api");
        assert_eq!(parsed["info"]["version"], "0.0.1");
    }

    #[tokio::test]
    async fn respects_with_path_override() {
        let plugin = OpenApiPlugin::new(ApiDoc::openapi()).with_path("/api/spec");
        let (router, _) = bootstrap().http_plugin(plugin).into_router().unwrap();

        let res = router
            .oneshot(HReq::get("/api/spec").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // default path is *not* mounted when overridden
        let (router2, _) = bootstrap()
            .http_plugin(OpenApiPlugin::new(ApiDoc::openapi()).with_path("/api/spec"))
            .into_router()
            .unwrap();
        let res = router2
            .oneshot(HReq::get("/openapi.json").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
