//! Phase-keyword middleware for plugins.
//!
//! [`HttpPlugin::middleware`](crate::HttpPlugin::middleware) returns a
//! `Vec<MiddlewareEntry>`. Each entry is tagged with a
//! [`MiddlewarePhase`] so `bootstrap()` can fold them into the app
//! router in the right layering order.
//!
//! Phase mapping (request-flow order, outermost first):
//!
//! 1. `BeforeGlobal`  — runs first on the request, last on the response
//! 2. `AfterGlobal`
//! 3. *(framework)* `Extension(Container)` + contributor pipeline
//! 4. `BeforeRoutes`
//! 5. `AfterRoutes`   — innermost, wraps the handler directly
//! 6. handler
//!
//! axum applies layers outermost-first as they're added in reverse — so
//! bootstrap installs them in `AfterRoutes → BeforeRoutes → framework →
//! AfterGlobal → BeforeGlobal` order. The resulting request flow matches
//! KickJS's `beforeGlobal / afterGlobal / beforeRoutes / afterRoutes`
//! semantics.

use axum::Router;
use std::sync::Arc;

/// Lifecycle phase at which a piece of middleware runs.
///
/// Maps 1-1 to KickJS adapter / plugin `middleware()` phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MiddlewarePhase {
    /// Outermost — request enters here first, response leaves here
    /// last. Use for request-id generation, top-level tracing,
    /// security headers that wrap every other layer.
    BeforeGlobal,
    /// After `BeforeGlobal`, still before the framework's container +
    /// contributor pipeline. Typical: body limits, request logging
    /// that wants to see the post-`BeforeGlobal` view.
    AfterGlobal,
    /// After the framework layers, before handlers. Typical: auth gates
    /// that need DI / contributor outputs available.
    BeforeRoutes,
    /// Innermost — wraps the handler directly. Typical: response
    /// post-processing, per-handler instrumentation.
    AfterRoutes,
}

type ApplyFn = Arc<dyn Fn(Router) -> Router + Send + Sync>;

/// A phase-tagged middleware contributed by a plugin.
///
/// Construct via [`MiddlewareEntry::from_async_fn`] for axum-style
/// `async fn(Request, Next) -> Response` middleware, or
/// [`MiddlewareEntry::router_layer`] for arbitrary
/// `Fn(Router) -> Router` transformations (lets you attach any tower
/// `Layer` with a one-liner like
/// `|r| r.layer(TraceLayer::new_for_http())`).
#[derive(Clone)]
pub struct MiddlewareEntry {
    phase: MiddlewarePhase,
    apply: ApplyFn,
}

impl MiddlewareEntry {
    /// Phase this entry runs at.
    pub fn phase(&self) -> MiddlewarePhase {
        self.phase
    }

    /// Apply the entry to `router`, returning the wrapped router.
    /// Called once per entry at bootstrap.
    pub fn apply(&self, router: Router) -> Router {
        (self.apply)(router)
    }

    /// Build an entry from an axum-style middleware closure.
    ///
    /// ```ignore
    /// MiddlewareEntry::from_async_fn(MiddlewarePhase::BeforeGlobal, |req, next| async move {
    ///     let mut res = next.run(req).await;
    ///     res.headers_mut().insert("x-served-by", "kick-rs".parse().unwrap());
    ///     res
    /// })
    /// ```
    pub fn from_async_fn<F, Fut>(phase: MiddlewarePhase, f: F) -> Self
    where
        F: Fn(axum::extract::Request, axum::middleware::Next) -> Fut
            + Clone
            + Send
            + Sync
            + 'static,
        Fut: std::future::Future<Output = axum::response::Response> + Send + 'static,
    {
        let apply: ApplyFn =
            Arc::new(move |r: Router| r.layer(axum::middleware::from_fn(f.clone())));
        Self { phase, apply }
    }

    /// Build an entry from an arbitrary `Fn(Router) -> Router`
    /// transformation. Useful for attaching any tower
    /// [`Layer`](tower::Layer) via
    /// `Router::layer`:
    ///
    /// ```ignore
    /// MiddlewareEntry::router_layer(MiddlewarePhase::BeforeGlobal, |r|
    ///     r.layer(tower_http::cors::CorsLayer::permissive())
    /// )
    /// ```
    pub fn router_layer<F>(phase: MiddlewarePhase, apply_fn: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        Self {
            phase,
            apply: Arc::new(apply_fn),
        }
    }
}

impl std::fmt::Debug for MiddlewareEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiddlewareEntry")
            .field("phase", &self.phase)
            .finish()
    }
}

/// Bucket helper: split a flat list of entries by phase so the
/// bootstrap loop can apply them in the right order without four
/// passes over the input.
pub(crate) fn group_by_phase(entries: Vec<MiddlewareEntry>) -> PhaseBuckets {
    let mut b = PhaseBuckets::default();
    for entry in entries {
        match entry.phase {
            MiddlewarePhase::BeforeGlobal => b.before_global.push(entry),
            MiddlewarePhase::AfterGlobal => b.after_global.push(entry),
            MiddlewarePhase::BeforeRoutes => b.before_routes.push(entry),
            MiddlewarePhase::AfterRoutes => b.after_routes.push(entry),
        }
    }
    b
}

#[derive(Default)]
pub(crate) struct PhaseBuckets {
    pub(crate) before_global: Vec<MiddlewareEntry>,
    pub(crate) after_global: Vec<MiddlewareEntry>,
    pub(crate) before_routes: Vec<MiddlewareEntry>,
    pub(crate) after_routes: Vec<MiddlewareEntry>,
}
