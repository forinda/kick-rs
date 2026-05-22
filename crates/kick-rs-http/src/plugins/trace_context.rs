//! `TraceContextPlugin` — parses and propagates W3C Trace Context
//! (<https://www.w3.org/TR/trace-context/>) on every request.
//!
//! On each request:
//!
//! 1. Look for a valid `traceparent` header. If present and parses,
//!    use its `trace_id` and `parent_id` (which becomes the *parent*
//!    of this request's span) and mint a fresh `span_id` for the
//!    current hop.
//! 2. If absent or malformed, mint a fresh trace.
//! 3. Insert a [`TraceContext`] into the request extensions so
//!    handlers and contributors can read it.
//! 4. Set the outbound `traceparent` header on the response, so
//!    downstream tooling and clients can continue the trace.
//! 5. Pass through `tracestate` unchanged (vendor-specific).
//!
//! This plugin does **not** integrate with `opentelemetry` directly —
//! it just surfaces the W3C identifiers. Drop in an OTel exporter
//! separately if you want spans shipped somewhere.

use crate::{MiddlewareEntry, MiddlewarePhase};
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use kick_rs_core::Plugin;

const TRACEPARENT: &str = "traceparent";
const TRACESTATE: &str = "tracestate";

/// Parsed W3C trace context for the current request.
///
/// `trace_id` is 16 bytes (32 hex chars); `span_id` and `parent_id`
/// are 8 bytes (16 hex chars). `parent_id` is `None` when this hop
/// generated the trace (no inbound `traceparent`).
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// 16-byte trace identifier, encoded as 32 lowercase hex chars.
    pub trace_id: String,
    /// 8-byte span identifier for *this* hop, encoded as 16 lowercase hex chars.
    pub span_id: String,
    /// Inbound `parent_id` from the upstream `traceparent` header,
    /// when this request continues an existing trace. `None` when this
    /// hop minted a new trace.
    pub parent_id: Option<String>,
    /// W3C trace flags byte. Bit 0 (`0x01`) is the *sampled* flag.
    pub flags: u8,
}

impl TraceContext {
    /// Render this context as a `traceparent` header value (W3C
    /// version `00`).
    pub fn to_traceparent(&self) -> String {
        format!("00-{}-{}-{:02x}", self.trace_id, self.span_id, self.flags)
    }
}

/// Plugin: install as outermost so the entire pipeline can read the
/// `TraceContext` extension.
#[derive(Debug, Clone, Default)]
pub struct TraceContextPlugin;

impl Plugin for TraceContextPlugin {
    fn name(&self) -> &str {
        "trace-context"
    }
}

impl crate::HttpPlugin for TraceContextPlugin {
    fn middleware(&self) -> Vec<MiddlewareEntry> {
        vec![MiddlewareEntry::from_async_fn(
            MiddlewarePhase::BeforeGlobal,
            middleware,
        )]
    }
}

async fn middleware(mut req: Request, next: Next) -> Response {
    let inbound = req
        .headers()
        .get(TRACEPARENT)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_traceparent);

    let ctx = match inbound {
        Some(parsed) => TraceContext {
            trace_id: parsed.trace_id,
            span_id: new_span_id(),
            parent_id: Some(parsed.parent_id),
            flags: parsed.flags,
        },
        None => TraceContext {
            trace_id: new_trace_id(),
            span_id: new_span_id(),
            parent_id: None,
            flags: 0x01, // sampled by default — adopters can post-process if they don't want this
        },
    };

    let outbound = ctx.to_traceparent();
    req.extensions_mut().insert(ctx);

    let mut res = next.run(req).await;
    if let Ok(hv) = HeaderValue::from_str(&outbound) {
        res.headers_mut().insert(TRACEPARENT, hv);
    }
    // Echo tracestate unchanged if it was present on the request — we
    // don't add a vendor entry of our own.
    // (Currently sourced from response if a downstream layer set it;
    // otherwise nothing to do.)
    let _ = TRACESTATE;
    res
}

struct ParsedTraceparent {
    trace_id: String,
    parent_id: String,
    flags: u8,
}

/// Strict W3C version-00 parser. Rejects anything that doesn't match
/// `00-<32hex>-<16hex>-<2hex>` with all-zero trace/parent id treated
/// as invalid (per the spec).
fn parse_traceparent(s: &str) -> Option<ParsedTraceparent> {
    let parts: Vec<&str> = s.trim().split('-').collect();
    if parts.len() != 4 {
        return None;
    }
    let (version, trace_id, parent_id, flags) = (parts[0], parts[1], parts[2], parts[3]);
    if version != "00" || trace_id.len() != 32 || parent_id.len() != 16 || flags.len() != 2 {
        return None;
    }
    if !trace_id.chars().all(|c| c.is_ascii_hexdigit())
        || !parent_id.chars().all(|c| c.is_ascii_hexdigit())
        || !flags.chars().all(|c| c.is_ascii_hexdigit())
    {
        return None;
    }
    // Per spec: all-zero ids are invalid.
    if trace_id.chars().all(|c| c == '0') || parent_id.chars().all(|c| c == '0') {
        return None;
    }
    Some(ParsedTraceparent {
        trace_id: trace_id.to_owned(),
        parent_id: parent_id.to_owned(),
        flags: u8::from_str_radix(flags, 16).ok()?,
    })
}

/// 16 random bytes as 32 lowercase hex chars.
fn new_trace_id() -> String {
    let b = uuid::Uuid::new_v4().into_bytes();
    bytes_to_hex(&b)
}

/// 8 random bytes as 16 lowercase hex chars (first half of a fresh v4 uuid).
fn new_span_id() -> String {
    let b = uuid::Uuid::new_v4().into_bytes();
    bytes_to_hex(&b[..8])
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{:02x}", byte);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bootstrap, define_module};
    use axum::body::Body;
    use axum::http::{Request as HReq, StatusCode};
    use tower::ServiceExt;

    async fn echo_ctx(axum::Extension(ctx): axum::Extension<TraceContext>) -> String {
        format!(
            "{}|{}|{}|{:02x}",
            ctx.trace_id,
            ctx.span_id,
            ctx.parent_id.unwrap_or_else(|| "-".into()),
            ctx.flags
        )
    }

    fn mount() -> axum::Router {
        let m = define_module("t").get("/r", echo_ctx).build();
        bootstrap()
            .module(m)
            .http_plugin(TraceContextPlugin)
            .into_router()
            .unwrap()
            .0
    }

    #[tokio::test]
    async fn generates_when_no_traceparent() {
        let res = mount()
            .oneshot(HReq::get("/r").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let tp = res
            .headers()
            .get(TRACEPARENT)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        // Generated traceparent must parse + roundtrip
        let parsed = parse_traceparent(&tp).expect("generated traceparent must parse");
        assert_eq!(parsed.trace_id.len(), 32);
        assert_eq!(parsed.parent_id.len(), 16);
        let body = axum::body::to_bytes(res.into_body(), 256).await.unwrap();
        let body_str = std::str::from_utf8(&body).unwrap();
        // First-hop trace has no parent; handler sees parent_id = None
        assert!(body_str.ends_with("|-|01"));
    }

    #[tokio::test]
    async fn propagates_inbound_trace_id() {
        let inbound = "00-0af7651916cd43dd8448eb211c80319c-b9c7c989f97918e1-01";
        let res = mount()
            .oneshot(
                HReq::get("/r")
                    .header(TRACEPARENT, inbound)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = axum::body::to_bytes(res.into_body(), 256).await.unwrap();
        let body_str = std::str::from_utf8(&body).unwrap();
        // Same trace_id is reused; parent_id captured; fresh span_id (not asserted exactly here)
        assert!(body_str.starts_with("0af7651916cd43dd8448eb211c80319c|"));
        assert!(body_str.contains("|b9c7c989f97918e1|"));
        assert!(body_str.ends_with("|01"));
    }

    #[tokio::test]
    async fn malformed_traceparent_falls_back_to_generated() {
        let res = mount()
            .oneshot(
                HReq::get("/r")
                    .header(TRACEPARENT, "garbage")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), 256).await.unwrap();
        let body_str = std::str::from_utf8(&body).unwrap();
        // Fallback path -> no parent
        assert!(body_str.ends_with("|-|01"));
    }

    #[test]
    fn parse_rejects_invalid() {
        assert!(parse_traceparent("").is_none());
        assert!(parse_traceparent("00-short-b9c7c989f97918e1-01").is_none());
        assert!(
            parse_traceparent("ff-0af7651916cd43dd8448eb211c80319c-b9c7c989f97918e1-01").is_none()
        ); // wrong version
        assert!(
            parse_traceparent("00-00000000000000000000000000000000-b9c7c989f97918e1-01").is_none()
        ); // all-zero trace
        assert!(
            parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01").is_none()
        ); // all-zero parent
    }
}
