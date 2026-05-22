//! `HelmetPlugin` — sets a baseline of security-related response
//! headers. Modeled on the Node.js `helmet` package, but using
//! defaults that are sensible for a *Rust API* (rather than a
//! server-rendered HTML app).
//!
//! Defaults applied (all overridable via [`HelmetPlugin::builder`]):
//!
//! | Header                          | Default value                                   |
//! |---------------------------------|-------------------------------------------------|
//! | `X-Content-Type-Options`        | `nosniff`                                       |
//! | `X-Frame-Options`               | `DENY`                                          |
//! | `Strict-Transport-Security`     | `max-age=15552000; includeSubDomains`           |
//! | `Referrer-Policy`               | `no-referrer`                                   |
//! | `X-XSS-Protection`              | `0` (per OWASP — legacy header, disable)        |
//! | `Cross-Origin-Opener-Policy`    | `same-origin`                                   |
//! | `Cross-Origin-Resource-Policy`  | `same-origin`                                   |
//! | `Permissions-Policy`            | `accelerometer=(), camera=(), geolocation=(), microphone=()` |
//! | `Content-Security-Policy`       | *not set by default* — opt in via the builder, since CSP for an API is usually wrong |
//!
//! All headers are written in [`MiddlewarePhase::BeforeGlobal`] but
//! attached to the *response*, so they apply to every route uniformly.

use crate::{MiddlewareEntry, MiddlewarePhase};
use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use kick_rs_core::Plugin;
use std::sync::Arc;

#[derive(Clone)]
struct HelmetHeaders {
    pairs: Vec<(HeaderName, HeaderValue)>,
}

/// Security-headers plugin. See module docs for the default header set.
#[derive(Clone)]
pub struct HelmetPlugin {
    headers: Arc<HelmetHeaders>,
}

/// Builder for customising the header set written by [`HelmetPlugin`].
///
/// Start from [`HelmetPlugin::builder`] (defaults pre-populated) or
/// [`HelmetBuilder::empty`] (no headers — opt-in everything).
pub struct HelmetBuilder {
    pairs: Vec<(HeaderName, HeaderValue)>,
}

impl HelmetBuilder {
    /// No headers — opt-in everything from here.
    pub fn empty() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Pre-populated with the defaults documented at the module level.
    pub fn with_defaults() -> Self {
        Self::empty()
            .set("X-Content-Type-Options", "nosniff")
            .set("X-Frame-Options", "DENY")
            .set(
                "Strict-Transport-Security",
                "max-age=15552000; includeSubDomains",
            )
            .set("Referrer-Policy", "no-referrer")
            .set("X-XSS-Protection", "0")
            .set("Cross-Origin-Opener-Policy", "same-origin")
            .set("Cross-Origin-Resource-Policy", "same-origin")
            .set(
                "Permissions-Policy",
                "accelerometer=(), camera=(), geolocation=(), microphone=()",
            )
    }

    /// Override or insert a header. Silently skips invalid names/values
    /// — the builder is intended for static, programmer-controlled input.
    pub fn set(mut self, name: &str, value: &str) -> Self {
        if let (Ok(n), Ok(v)) = (HeaderName::try_from(name), HeaderValue::try_from(value)) {
            // Replace if already present, otherwise append.
            if let Some(slot) = self.pairs.iter_mut().find(|(hn, _)| *hn == n) {
                slot.1 = v;
            } else {
                self.pairs.push((n, v));
            }
        }
        self
    }

    /// Remove a header from the set (e.g. drop HSTS when running over HTTP locally).
    pub fn remove(mut self, name: &str) -> Self {
        if let Ok(n) = HeaderName::try_from(name) {
            self.pairs.retain(|(hn, _)| *hn != n);
        }
        self
    }

    /// Set a `Content-Security-Policy`. Off by default — most JSON APIs
    /// don't need one and a wrong CSP can break unrelated tooling.
    pub fn content_security_policy(self, policy: &str) -> Self {
        self.set("Content-Security-Policy", policy)
    }

    /// Finalize the builder into a [`HelmetPlugin`].
    pub fn build(self) -> HelmetPlugin {
        HelmetPlugin {
            headers: Arc::new(HelmetHeaders { pairs: self.pairs }),
        }
    }
}

impl HelmetPlugin {
    /// Builder pre-loaded with the documented defaults.
    pub fn builder() -> HelmetBuilder {
        HelmetBuilder::with_defaults()
    }
}

impl Default for HelmetPlugin {
    fn default() -> Self {
        HelmetBuilder::with_defaults().build()
    }
}

impl std::fmt::Debug for HelmetPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelmetPlugin")
            .field("headers", &self.headers.pairs.len())
            .finish()
    }
}

impl Plugin for HelmetPlugin {
    fn name(&self) -> &str {
        "helmet"
    }
}

async fn apply(headers: Arc<HelmetHeaders>, req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    for (name, value) in &headers.pairs {
        // Don't clobber if the handler/route already set the header itself.
        h.entry(name).or_insert_with(|| value.clone());
    }
    res
}

impl crate::HttpPlugin for HelmetPlugin {
    fn middleware(&self) -> Vec<MiddlewareEntry> {
        let headers = self.headers.clone();
        vec![MiddlewareEntry::from_async_fn(
            MiddlewarePhase::BeforeGlobal,
            move |req, next| {
                let h = headers.clone();
                Box::pin(async move { apply(h, req, next).await })
            },
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bootstrap, define_module};
    use axum::body::Body;
    use axum::http::{Request as HReq, StatusCode};
    use tower::ServiceExt;

    async fn ok() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn writes_default_security_headers() {
        let m = define_module("t").get("/r", ok).build();
        let (router, _) = bootstrap()
            .http_plugin(HelmetPlugin::default())
            .module(m)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(HReq::get("/r").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let h = res.headers();
        assert_eq!(h.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(h.get("x-frame-options").unwrap(), "DENY");
        assert_eq!(h.get("referrer-policy").unwrap(), "no-referrer");
        assert!(h.get("strict-transport-security").is_some());
        assert!(h.get("permissions-policy").is_some());
        // CSP is not in the default set — APIs opt in explicitly.
        assert!(h.get("content-security-policy").is_none());
    }

    #[tokio::test]
    async fn builder_can_override_and_remove() {
        let plugin = HelmetPlugin::builder()
            .remove("Strict-Transport-Security")
            .content_security_policy("default-src 'none'")
            .set("X-Frame-Options", "SAMEORIGIN")
            .build();

        let m = define_module("t").get("/r", ok).build();
        let (router, _) = bootstrap()
            .http_plugin(plugin)
            .module(m)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(HReq::get("/r").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let h = res.headers();
        assert!(h.get("strict-transport-security").is_none());
        assert_eq!(h.get("x-frame-options").unwrap(), "SAMEORIGIN");
        assert_eq!(
            h.get("content-security-policy").unwrap(),
            "default-src 'none'"
        );
    }

    #[tokio::test]
    async fn does_not_clobber_handler_set_headers() {
        async fn custom_xfo() -> axum::response::Response {
            let mut r = axum::response::Response::new(Body::from("ok"));
            r.headers_mut().insert(
                "X-Frame-Options",
                HeaderValue::from_static("ALLOW-FROM https://trusted.example"),
            );
            r
        }

        let m = define_module("t").get("/r", custom_xfo).build();
        let (router, _) = bootstrap()
            .http_plugin(HelmetPlugin::default())
            .module(m)
            .into_router()
            .unwrap();

        let res = router
            .oneshot(HReq::get("/r").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // Handler-set value wins; helmet only fills missing headers.
        assert_eq!(
            res.headers().get("x-frame-options").unwrap(),
            "ALLOW-FROM https://trusted.example"
        );
    }
}
