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

use crate::adapter::BuildContext;
use crate::container::ContainerBuilder;
use crate::contributor::AnyContributor;
use crate::error::KickResult;
use std::marker::PhantomData;

/// A packaged bundle of DI providers, tower layers, and contributors.
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

    /// Context contributors to add to the pipeline.
    fn contributors(&self) -> Vec<AnyContributor> {
        Vec::new()
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
