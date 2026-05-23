//! HTTP-aware extension to the core [`Plugin`] trait.
//!
//! Plugins that need to ship handler routes implement [`HttpPlugin`]
//! (which extends [`Plugin`] with one extra
//! method — [`HttpPlugin::http_modules`]) rather than `Plugin` directly.
//! Bootstrap accepts both via [`Bootstrap::plugin`](crate::Bootstrap::plugin)
//! and [`Bootstrap::http_plugin`](crate::Bootstrap::http_plugin); the
//! aggregated set is folded into the app router alongside user-provided
//! modules.

use crate::module::HttpModule;
use kick_rs_core::Plugin;

/// HTTP-aware plugin trait. Same lifecycle as the core
/// [`Plugin`], plus the ability to contribute
/// full [`HttpModule`]s (routes + handlers + per-module providers and
/// contributors).
///
/// ```ignore
/// struct AuthPlugin { secret: String }
///
/// impl Plugin for AuthPlugin {
///     fn name(&self) -> &str { "auth" }
///     fn contributors(&self) -> Vec<AnyContributor> {
///         vec![erase_contributor(LoadCurrentUser)]
///     }
///     fn adapters(&self) -> Vec<Arc<dyn Adapter>> {
///         vec![Arc::new(JwtVerifierAdapter::new(&self.secret))]
///     }
/// }
///
/// impl HttpPlugin for AuthPlugin {
///     fn http_modules(&self) -> Vec<HttpModule> {
///         vec![
///             define_module("auth")
///                 .prefix("/auth")
///                 .post("/login", handlers::login)
///                 .post("/refresh", handlers::refresh)
///                 .build()
///         ]
///     }
/// }
///
/// bootstrap().http_plugin(AuthPlugin { secret: env.auth_secret })
/// ```
pub trait HttpPlugin: Plugin {
    /// HTTP modules this plugin contributes. Each module's routes are
    /// mounted onto the app router; its providers and contributors are
    /// folded into the container and pipeline. Sub-modules are
    /// supported recursively.
    fn http_modules(&self) -> Vec<HttpModule> {
        Vec::new()
    }

    /// Phase-keyword middleware this plugin contributes. See
    /// [`MiddlewarePhase`](crate::MiddlewarePhase) for the four phase
    /// keywords and the order in which they wrap the router.
    ///
    /// ```ignore
    /// fn middleware(&self) -> Vec<MiddlewareEntry> {
    ///     vec![
    ///         MiddlewareEntry::from_async_fn(MiddlewarePhase::BeforeGlobal,
    ///             |req, next| async move {
    ///                 let mut res = next.run(req).await;
    ///                 res.headers_mut().insert("x-served-by", "kick-rs".parse().unwrap());
    ///                 res
    ///             }),
    ///     ]
    /// }
    /// ```
    fn middleware(&self) -> Vec<crate::MiddlewareEntry> {
        Vec::new()
    }

    /// Path prefixes the contributor pipeline should skip for routes
    /// this plugin owns.
    ///
    /// The default is empty — user-mounted modules always go through
    /// the contributor chain so request-scoped deps (auth, tenant,
    /// trace id) are populated. Framework-owned plugins (OpenAPI,
    /// DevTools, Assets) override this so their routes don't 500
    /// when the adopter's contributors require headers the framework
    /// route can't supply (think `LoadTenant` requiring
    /// `x-tenant-slug` — that contributor's not relevant for
    /// `GET /openapi.json`).
    ///
    /// Matching is prefix-based: an entry of `/static` matches both
    /// `/static` and `/static/app.js`.
    fn bypass_contributor_paths(&self) -> Vec<String> {
        Vec::new()
    }
}
