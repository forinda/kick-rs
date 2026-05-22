//! `RequestIdPlugin` — propagates `X-Request-Id` headers and stashes
//! a typed [`RequestId`] in the request extensions.
//!
//! If the incoming request carries `X-Request-Id`, it's used as-is.
//! Otherwise a fresh UUIDv7 is generated. The id is also written to
//! the response `X-Request-Id` header so clients can correlate.
//!
//! Handlers / contributors that want it as a contributor output can
//! declare `Deps = (HeaderMap,)` and read `x-request-id` directly, or
//! pull `RequestId` via an axum `Extension<RequestId>` extractor.

use crate::{MiddlewareEntry, MiddlewarePhase};
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use kick_rs_core::Plugin;

const HEADER: &str = "x-request-id";

/// Cloneable wrapper around the generated/propagated request id.
/// Inserted as a request extension; pull via
/// `axum::Extension<RequestId>` in a handler.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// `X-Request-Id` plugin. Mount as outermost middleware so every
/// downstream layer sees the id.
#[derive(Debug, Clone, Default)]
pub struct RequestIdPlugin;

impl Plugin for RequestIdPlugin {
    fn name(&self) -> &str {
        "request-id"
    }

    fn introspect(&self) -> Option<kick_rs_core::IntrospectionSnapshot> {
        // First built-in plugin to opt into the DevTools snapshot.
        // The state is intentionally static — there's no per-request
        // tracking on this plugin — but a non-empty body still tells
        // adopters that the plugin is wired up correctly.
        Some(kick_rs_core::IntrospectionSnapshot {
            protocol_version: 1,
            kind: kick_rs_core::IntrospectionKind::Plugin,
            name: self.name().to_owned(),
            state: serde_json::json!({
                "header": HEADER,
                "id_format": "uuid-v7-or-passthrough",
            }),
            tokens: vec!["RequestId".to_owned()],
            memory_bytes: None,
        })
    }
}

impl crate::HttpPlugin for RequestIdPlugin {
    fn middleware(&self) -> Vec<MiddlewareEntry> {
        vec![MiddlewareEntry::from_async_fn(
            MiddlewarePhase::BeforeGlobal,
            middleware,
        )]
    }
}

async fn middleware(mut req: Request, next: Next) -> Response {
    // Reuse the inbound id when present; otherwise mint a fresh v7.
    let id = req
        .headers()
        .get(HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    // Echo the canonical value back into the request so contributors
    // declaring `Deps = (HeaderMap,)` see it.
    if let Ok(hv) = HeaderValue::from_str(&id) {
        req.headers_mut().insert(HEADER, hv.clone());
        req.extensions_mut().insert(RequestId(id.clone()));

        let mut res = next.run(req).await;
        res.headers_mut().insert(HEADER, hv);
        res
    } else {
        // Parse fail (extremely unusual — bad UUID? bad upstream header?):
        // run the request without injecting, but log a warning so it's
        // not silently dropped.
        tracing::warn!(target: "kick-rs::request-id", id = %id, "could not coerce request id into a header value");
        next.run(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bootstrap, define_module};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn echo_id(axum::Extension(id): axum::Extension<RequestId>) -> String {
        id.0
    }

    #[tokio::test]
    async fn generates_id_when_missing() {
        let m = define_module("t").get("/r", echo_id).build();
        let (router, _) = bootstrap()
            .module(m)
            .http_plugin(RequestIdPlugin)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(Request::get("/r").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let got_header = res
            .headers()
            .get(HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
        let body = axum::body::to_bytes(res.into_body(), 256).await.unwrap();
        let body_str = std::str::from_utf8(&body).unwrap().to_owned();
        // Generated id is echoed back as both the response header and
        // the handler's view of `RequestId`.
        assert_eq!(got_header.as_deref(), Some(body_str.as_str()));
        assert!(!body_str.is_empty());
    }

    #[tokio::test]
    async fn propagates_inbound_id() {
        let m = define_module("t").get("/r", echo_id).build();
        let (router, _) = bootstrap()
            .module(m)
            .http_plugin(RequestIdPlugin)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(
                Request::get("/r")
                    .header(HEADER, "client-supplied-42")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers().get(HEADER).unwrap(), "client-supplied-42");
        let body = axum::body::to_bytes(res.into_body(), 256).await.unwrap();
        assert_eq!(&body[..], b"client-supplied-42");
    }
}
