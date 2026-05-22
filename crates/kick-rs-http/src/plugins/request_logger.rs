//! `RequestLoggerPlugin` — emits a `tracing::info` log line per request
//! with method, path, status, and elapsed time. Pairs naturally with
//! [`RequestIdPlugin`](super::request_id::RequestIdPlugin) — when both
//! are mounted (request-id first / outer), the logger picks up the
//! request id from the request extensions automatically and includes
//! it in the structured log.
//!
//! Designed as a thin, dependency-free wrapper around `tracing`.
//! Adopters who want JSON logging or richer fields can run the standard
//! `tracing-subscriber` JSON layer at startup — this plugin just emits
//! events, it doesn't dictate format.

use crate::{MiddlewareEntry, MiddlewarePhase};
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use kick_rs_core::Plugin;
use std::time::Instant;

/// Per-request logger plugin. Mount as outermost (after request-id, if
/// using one) so it sees the canonical request id.
#[derive(Debug, Clone, Default)]
pub struct RequestLoggerPlugin;

impl Plugin for RequestLoggerPlugin {
    fn name(&self) -> &str {
        "request-logger"
    }
}

impl crate::HttpPlugin for RequestLoggerPlugin {
    fn middleware(&self) -> Vec<MiddlewareEntry> {
        vec![MiddlewareEntry::from_async_fn(
            MiddlewarePhase::BeforeGlobal,
            middleware,
        )]
    }
}

async fn middleware(req: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = req.method().clone();
    let uri = req.uri().clone();

    // Capture the request id, if any, BEFORE handing the request off
    // to the inner stack. We can't borrow extensions back out of the
    // response on the way out.
    #[cfg(feature = "plugin-request-id")]
    let request_id = req
        .extensions()
        .get::<super::request_id::RequestId>()
        .map(|r| r.0.clone());

    let res = next.run(req).await;
    let elapsed_us = started.elapsed().as_micros();
    let status = res.status().as_u16();

    #[cfg(feature = "plugin-request-id")]
    {
        if let Some(id) = request_id {
            tracing::info!(
                target: "kick-rs::request",
                request_id = %id,
                method = %method,
                path = uri.path(),
                status,
                elapsed_us,
                "request"
            );
            return res;
        }
    }

    tracing::info!(
        target: "kick-rs::request",
        method = %method,
        path = uri.path(),
        status,
        elapsed_us,
        "request"
    );
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bootstrap, define_module};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn ok() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn logger_does_not_break_the_pipeline() {
        // Functional smoke — the tracing emission is captured by
        // tracing-subscriber in adopter code; here we just verify the
        // plugin doesn't break the request.
        let m = define_module("t").get("/r", ok).build();
        let (router, _) = bootstrap()
            .http_plugin(RequestLoggerPlugin)
            .module(m)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(Request::get("/r").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 64).await.unwrap();
        assert_eq!(&body[..], b"ok");
    }
}
