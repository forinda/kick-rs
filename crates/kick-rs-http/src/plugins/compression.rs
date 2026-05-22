//! `CompressionPlugin` — wraps [`tower_http::compression::CompressionLayer`]
//! as an `HttpPlugin`. Negotiates gzip / br / deflate / zstd response
//! compression based on the request's `Accept-Encoding` header.
//!
//! Pulled in by the `plugin-compression` feature, which in turn enables
//! `tower-http/compression-full` (covers all four algorithms). Adopters
//! who want a slimmer dep set can opt out via `default-features = false`
//! and pull only what they need from `tower-http` directly.

use crate::{MiddlewareEntry, MiddlewarePhase};
use kick_rs_core::Plugin;
use tower_http::compression::CompressionLayer;

/// Compression plugin. Installed at
/// [`MiddlewarePhase::AfterGlobal`] so it sits between the global
/// concerns (CORS, request-id) and the per-route layers. This wraps
/// the response body before per-route layers can short-circuit it.
#[derive(Clone, Default)]
pub struct CompressionPlugin {
    layer: CompressionLayer,
}

impl CompressionPlugin {
    /// Drop in a fully-configured `CompressionLayer`.
    pub fn with_layer(layer: CompressionLayer) -> Self {
        Self { layer }
    }
}

impl std::fmt::Debug for CompressionPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompressionPlugin").finish()
    }
}

impl Plugin for CompressionPlugin {
    fn name(&self) -> &str {
        "compression"
    }
}

impl crate::HttpPlugin for CompressionPlugin {
    fn middleware(&self) -> Vec<MiddlewareEntry> {
        let layer = self.layer.clone();
        vec![MiddlewareEntry::router_layer(
            MiddlewarePhase::AfterGlobal,
            move |r| r.layer(layer.clone()),
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bootstrap, define_module};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    // A payload large enough that compression negotiation kicks in.
    async fn payload() -> String {
        "x".repeat(2048)
    }

    #[tokio::test]
    async fn compresses_when_accept_encoding_requests_gzip() {
        let m = define_module("t").get("/p", payload).build();
        let (router, _) = bootstrap()
            .http_plugin(CompressionPlugin::default())
            .module(m)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(
                Request::get("/p")
                    .header("accept-encoding", "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        // tower-http sets content-encoding when it compresses.
        assert_eq!(
            res.headers()
                .get("content-encoding")
                .and_then(|v| v.to_str().ok()),
            Some("gzip")
        );
    }

    #[tokio::test]
    async fn skips_compression_when_not_requested() {
        let m = define_module("t").get("/p", payload).build();
        let (router, _) = bootstrap()
            .http_plugin(CompressionPlugin::default())
            .module(m)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(Request::get("/p").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("content-encoding").is_none());
    }
}
