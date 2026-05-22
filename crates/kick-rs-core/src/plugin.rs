//! Plugin trait + `define_plugin()` factory.
//!
//! Plugins are like adapters minus the long-lived lifecycle — pure DI
//! bindings + contributors today. See [`SPEC.md` §5](../SPEC.md#5-plugins-deep-dive).
//!
//! ## Current capabilities
//!
//! A `Plugin` can:
//! - Register DI providers via [`Plugin::register`]
//! - Contribute to the request-time context pipeline via
//!   [`Plugin::contributors`]
//!
//! ## Planned (post-Phase 4)
//!
//! Plugins are expected to grow into full mini-frameworks-in-a-package:
//! - **Mounting modules** — `Plugin::modules() -> Vec<HttpModule>` so a
//!   plugin can drop in handler routes + service wiring as a unit.
//! - **Owning adapters** — `Plugin::adapters() -> Vec<Arc<dyn Adapter>>`
//!   so a plugin can ship a connection pool or background worker.
//! - **Tower layers** — `Plugin::layers()` for cross-cutting middleware.
//!
//! Bootstrap will aggregate these alongside user-provided modules
//! and adapters, giving plugin authors the same composition primitives
//! as application code. Tracked as Phase 5 work.

use crate::adapter::{Adapter, BuildContext};
use crate::container::ContainerBuilder;
use crate::contributor::AnyContributor;
use crate::error::KickResult;
use crate::introspect::IntrospectionSnapshot;
use crate::module::Module;
use std::marker::PhantomData;
use std::sync::Arc;

/// A packaged bundle a plugin contributes to the application.
///
/// Every method has a sensible default so plugin authors only override
/// what they actually ship. The common shape:
///
/// ```ignore
/// struct MetricsPlugin;
/// impl Plugin for MetricsPlugin {
///     fn name(&self) -> &str { "metrics" }
///     fn adapters(&self) -> Vec<Arc<dyn Adapter>> { vec![Arc::new(MetricsAdapter::new())] }
///     fn contributors(&self) -> Vec<AnyContributor> { vec![erase_contributor(AttachRequestId)] }
/// }
/// ```
///
/// HTTP-specific extensions (handler routes, tower layers) live on
/// [`HttpPlugin`](../../kick_rs_http/trait.HttpPlugin.html) in the HTTP
/// crate; this trait stays transport-agnostic so non-HTTP transports
/// (queue workers, CLIs) can use the same plugin contract.
pub trait Plugin: Send + Sync + 'static {
    /// Stable name used for logging and `depends_on` lookups.
    fn name(&self) -> &str;

    /// Crate version — defaults to the implementing crate's version.
    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    /// Plugin names this one must mount after.
    fn depends_on(&self) -> &[&str] {
        &[]
    }

    /// Register DI providers into the (still mutable) container.
    fn register(&self, _builder: &mut ContainerBuilder) -> KickResult<()> {
        Ok(())
    }

    /// Context contributors this plugin ships. Aggregated with module-
    /// and bootstrap-level contributors into the per-app topo-sorted
    /// pipeline at boot.
    fn contributors(&self) -> Vec<AnyContributor> {
        Vec::new()
    }

    /// Adapters this plugin ships. Aggregated with bootstrap-level
    /// adapters and topo-sorted by `depends_on` at boot. Useful for
    /// plugins that bring their own connection pool, background worker,
    /// or observability exporter as a single unit.
    fn adapters(&self) -> Vec<Arc<dyn Adapter>> {
        Vec::new()
    }

    /// Transport-agnostic modules this plugin ships — providers +
    /// sub-module trees, no HTTP routes. Use this for shared service
    /// trees a plugin wants to surface without owning any handler
    /// endpoints. Plugins that ship HTTP routes should implement
    /// [`HttpPlugin`](../../kick_rs_http/trait.HttpPlugin.html) instead.
    fn modules(&self) -> Vec<Module> {
        Vec::new()
    }

    /// Post-startup task — runs once after the server is bound and
    /// `Adapter::after_start` has fired for every adapter. Useful for
    /// emitting "ready" log lines, warming caches, or pinging external
    /// systems. Errors are logged but don't abort the running server.
    ///
    /// Sequential across plugins; if you need fan-out, spawn inside.
    fn on_ready(
        &self,
        _container: &crate::Container,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = KickResult<()>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    /// Cooperative shutdown — runs once during the framework's graceful
    /// shutdown phase alongside `Adapter::shutdown`. Useful for plugins
    /// that own resources independent of any adapter.
    fn shutdown(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = KickResult<()>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    /// Optional state snapshot for DevTools / `cargo kick info`.
    /// Returning `Some(...)` enrolls this plugin in the
    /// `/__debug` introspection endpoint — the snapshot's `state`
    /// JSON shows up inline next to the plugin entry.
    ///
    /// Default `None` keeps plugins opted out by default so adopters
    /// don't accidentally leak internals to DevTools just by upgrading.
    fn introspect(&self) -> Option<IntrospectionSnapshot> {
        None
    }
}

/// Intermediate builder returned by [`define_plugin`].
pub struct PluginDef<C> {
    name: &'static str,
    defaults: Option<C>,
}

impl<C> std::fmt::Debug for PluginDef<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginDef")
            .field("name", &self.name)
            .field("has_defaults", &self.defaults.is_some())
            .finish()
    }
}

impl<C> PluginDef<C>
where
    C: Clone + Send + Sync + 'static,
{
    /// Default config used by `factory.call()`.
    pub fn defaults(mut self, c: C) -> Self {
        self.defaults = Some(c);
        self
    }

    /// Finalize the definition. The closure receives `(ctx, name, config)`
    /// and returns the concrete plugin. Plugins should store the supplied
    /// name as `String` since `.scoped()` will pass `"base:scope"`.
    pub fn build<P, F>(self, build_fn: F) -> PluginFactory<C, P, F>
    where
        P: Plugin,
        F: Fn(BuildContext, String, C) -> P + Send + Sync + 'static,
    {
        PluginFactory {
            name: self.name,
            defaults: self.defaults,
            build_fn,
            _phantom: PhantomData,
        }
    }
}

/// Concrete factory ready to mint plugin instances.
pub struct PluginFactory<C, P, F>
where
    C: Clone + Send + Sync + 'static,
    P: Plugin,
    F: Fn(BuildContext, String, C) -> P + Send + Sync + 'static,
{
    name: &'static str,
    defaults: Option<C>,
    build_fn: F,
    _phantom: PhantomData<fn() -> P>,
}

impl<C, P, F> PluginFactory<C, P, F>
where
    C: Clone + Send + Sync + 'static,
    P: Plugin,
    F: Fn(BuildContext, String, C) -> P + Send + Sync + 'static,
{
    /// Plugin name (without scope namespace).
    pub fn name(&self) -> &str {
        self.name
    }

    /// Whether a default config was provided.
    pub fn has_defaults(&self) -> bool {
        self.defaults.is_some()
    }

    /// Build an instance using the defaults. Panics if no defaults were
    /// set — use [`Self::with`] instead.
    pub fn call(&self) -> P {
        let cfg = self
            .defaults
            .clone()
            .expect("PluginFactory::call requires `.defaults(...)`; use `.with(config)` otherwise");
        (self.build_fn)(BuildContext, self.name.to_owned(), cfg)
    }

    /// Build an instance using a caller-supplied config.
    pub fn with(&self, config: C) -> P {
        (self.build_fn)(BuildContext, self.name.to_owned(), config)
    }

    /// Build an instance whose `name()` returns `"<base>:<scope>"`.
    pub fn scoped(&self, scope: &str, config: C) -> P {
        let scoped_name = format!("{}:{}", self.name, scope);
        (self.build_fn)(BuildContext, scoped_name, config)
    }
}

/// Begin defining a plugin. See module docs for example.
pub fn define_plugin<C>(name: &'static str) -> PluginDef<C>
where
    C: Clone + Send + Sync + 'static,
{
    PluginDef {
        name,
        defaults: None,
    }
}

// ──────────────────────────────── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Default)]
    struct CorsConfig {
        origins: Vec<String>,
    }

    struct CorsPlugin {
        name: String,
        cfg: CorsConfig,
    }
    impl Plugin for CorsPlugin {
        fn name(&self) -> &str {
            &self.name
        }
    }

    fn cors_def() -> PluginDef<CorsConfig> {
        define_plugin::<CorsConfig>("cors").defaults(CorsConfig::default())
    }

    #[test]
    fn call_uses_defaults_and_base_name() {
        let f = cors_def().build(|_ctx, name, cfg| CorsPlugin { name, cfg });
        let p = f.call();
        assert_eq!(p.name(), "cors");
        assert!(p.cfg.origins.is_empty());
    }

    #[test]
    fn with_overrides_config() {
        let f = cors_def().build(|_ctx, name, cfg| CorsPlugin { name, cfg });
        let p = f.with(CorsConfig {
            origins: vec!["https://app.example.com".into()],
        });
        assert_eq!(p.cfg.origins, vec!["https://app.example.com".to_string()]);
    }

    #[test]
    fn scoped_namespaces_name() {
        let f = cors_def().build(|_ctx, name, cfg| CorsPlugin { name, cfg });
        let p = f.scoped("admin", CorsConfig::default());
        assert_eq!(p.name(), "cors:admin");
    }

    #[test]
    #[should_panic(expected = ".defaults(...)")]
    fn call_without_defaults_panics() {
        let f = define_plugin::<CorsConfig>("nodefaults")
            .build(|_ctx, name, cfg| CorsPlugin { name, cfg });
        let _ = f.call();
    }

    #[test]
    fn default_trait_methods_return_sensible_values() {
        let f = cors_def().build(|_ctx, name, cfg| CorsPlugin { name, cfg });
        let p = f.call();
        assert_eq!(p.depends_on(), &[] as &[&str]);
        // version() returns the crate version — just assert it's non-empty.
        assert!(!p.version().is_empty());
    }
}
