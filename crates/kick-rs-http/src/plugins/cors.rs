//! `CorsPlugin` — wraps [`tower_http::cors::CorsLayer`] as an
//! `HttpPlugin` so adopters can drop CORS into the chain without
//! threading a tower layer through `bootstrap()` manually.
//!
//! Two construction paths:
//!
//! - [`CorsPlugin::permissive`] / [`CorsPlugin::very_permissive`] —
//!   matches the corresponding `tower_http::cors::CorsLayer` defaults
//!   for development-time use. Don't use in production without
//!   thinking about it.
//! - [`CorsPlugin::with_layer`] — drop in a fully-configured
//!   `CorsLayer` you built yourself. Use this for production where
//!   the origin allowlist matters.

use crate::{MiddlewareEntry, MiddlewarePhase};
use kick_rs_core::Plugin;
use tower_http::cors::CorsLayer;

/// CORS plugin. Installed at [`MiddlewarePhase::BeforeGlobal`] so
/// pre-flight `OPTIONS` requests short-circuit before any further
/// framework layer runs.
#[derive(Clone)]
pub struct CorsPlugin {
    layer: CorsLayer,
}

impl CorsPlugin {
    /// Construct from a user-supplied `CorsLayer`. Use this in
    /// production to bound the origin allowlist properly.
    pub fn with_layer(layer: CorsLayer) -> Self {
        Self { layer }
    }

    /// `CorsLayer::permissive()` — allows any origin / method / header.
    /// Convenient for development; **do not** use in production
    /// without further tightening.
    pub fn permissive() -> Self {
        Self {
            layer: CorsLayer::permissive(),
        }
    }

    /// `CorsLayer::very_permissive()` — strictly more permissive than
    /// [`Self::permissive`] (also reflects credentials). Almost
    /// certainly wrong in production.
    pub fn very_permissive() -> Self {
        Self {
            layer: CorsLayer::very_permissive(),
        }
    }
}

impl Default for CorsPlugin {
    fn default() -> Self {
        Self::permissive()
    }
}

impl std::fmt::Debug for CorsPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CorsPlugin").finish()
    }
}

impl Plugin for CorsPlugin {
    fn name(&self) -> &str {
        "cors"
    }
}

impl crate::HttpPlugin for CorsPlugin {
    fn middleware(&self) -> Vec<MiddlewareEntry> {
        let layer = self.layer.clone();
        vec![MiddlewareEntry::router_layer(
            MiddlewarePhase::BeforeGlobal,
            move |r| r.layer(layer.clone()),
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bootstrap, define_module};
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    async fn ok() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn preflight_returns_200_under_permissive() {
        let m = define_module("t").get("/r", ok).build();
        let (router, _) = bootstrap()
            .http_plugin(CorsPlugin::permissive())
            .module(m)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/r")
                    .header("origin", "https://example.com")
                    .header("access-control-request-method", "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // CorsLayer short-circuits OPTIONS preflight; CorsLayer::permissive
        // returns 200 with the appropriate ACA headers.
        assert!(
            res.status() == StatusCode::OK || res.status() == StatusCode::NO_CONTENT,
            "expected 200/204 preflight, got {}",
            res.status()
        );
        assert!(res.headers().get("access-control-allow-origin").is_some());
    }

    #[tokio::test]
    async fn actual_request_passes_through() {
        let m = define_module("t").get("/r", ok).build();
        let (router, _) = bootstrap()
            .http_plugin(CorsPlugin::permissive())
            .module(m)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(
                Request::get("/r")
                    .header("origin", "https://example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 16).await.unwrap();
        assert_eq!(&body[..], b"ok");
    }
}
