//! Adapter trait + `define_adapter()` factory.
//!
//! See [`SPEC.md` §4.4](../SPEC.md#44-adapter) for usage and
//! [`ARCHITECTURE.md` §4](../ARCHITECTURE.md#4-adapter-lifecycle--topo-sort)
//! for the lifecycle and topo-sort.
//!
//! ## Phase 1.3 scope
//!
//! - [`define_adapter`] returns an [`AdapterDef`] that builds into an
//!   [`AdapterFactory`] via `.defaults(...).build(...)`.
//! - The factory exposes `.call()`, `.with(config)`, `.scoped(scope, config)`.
//! - `.async_()` (lazy config resolution during `before_start`) lands later.

use crate::container::Container;
use crate::error::KickResult;
use async_trait::async_trait;
use std::marker::PhantomData;

/// A long-lived component with lifecycle hooks. Examples: Postgres pool,
/// OTel exporter, WebSocket hub.
#[async_trait]
pub trait Adapter: Send + Sync + 'static {
    /// Stable name used for logging and `depends_on` lookups.
    fn name(&self) -> &str;

    /// Adapter names this one must mount after.
    fn depends_on(&self) -> &[&str] {
        &[]
    }

    /// Runs before the container is sealed. Can mutate `ctx.container`.
    async fn before_mount(&self, _ctx: &AdapterContext) -> KickResult<()> {
        Ok(())
    }

    /// Runs after the container is sealed but before the server starts.
    async fn before_start(&self, _ctx: &AdapterContext) -> KickResult<()> {
        Ok(())
    }

    /// Runs once the server is accepting connections.
    async fn after_start(&self, _ctx: &AdapterContext) -> KickResult<()> {
        Ok(())
    }

    /// Cooperative shutdown — peers run concurrently under `tokio::join!`
    /// with a per-adapter timeout budget.
    async fn shutdown(&self) -> KickResult<()> {
        Ok(())
    }
}

/// Context passed to every adapter hook. Carries a read-side
/// [`Container`] reference so hooks can resolve siblings.
#[derive(Clone)]
pub struct AdapterContext {
    /// The application container.
    pub container: Container,
}

impl std::fmt::Debug for AdapterContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterContext")
            .field("container", &self.container)
            .finish()
    }
}

/// Build-time context handed to the closure passed to
/// [`AdapterDef::build`]. Reserved for future use (e.g., access to the
/// partial container, plugin registry inspection). Empty in Phase 1.3.
#[derive(Default, Debug, Clone, Copy)]
pub struct BuildContext;

// ─────────────────────────── Factory machinery ────────────────────────────

/// Intermediate builder returned by [`define_adapter`]. Set defaults then
/// `.build(...)` to produce an [`AdapterFactory`].
pub struct AdapterDef<C> {
    name: &'static str,
    defaults: Option<C>,
}

impl<C> std::fmt::Debug for AdapterDef<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterDef")
            .field("name", &self.name)
            .field("has_defaults", &self.defaults.is_some())
            .finish()
    }
}

impl<C> AdapterDef<C>
where
    C: Clone + Send + Sync + 'static,
{
    /// Default config used by `factory.call()`.
    pub fn defaults(mut self, c: C) -> Self {
        self.defaults = Some(c);
        self
    }

    /// Finalize the definition. The closure receives `(ctx, name, config)`
    /// and returns the concrete adapter. `name` will be the bare adapter
    /// name (or `base:scope` if produced via [`AdapterFactory::scoped`]),
    /// so adapter implementations should store it as `String`.
    pub fn build<A, F>(self, build_fn: F) -> AdapterFactory<C, A, F>
    where
        A: Adapter,
        F: Fn(BuildContext, String, C) -> A + Send + Sync + 'static,
    {
        AdapterFactory {
            name: self.name,
            defaults: self.defaults,
            build_fn,
            _phantom: PhantomData,
        }
    }
}

/// Concrete factory ready to mint adapter instances. Cheap to keep around;
/// stateless aside from the closure and defaults.
pub struct AdapterFactory<C, A, F>
where
    C: Clone + Send + Sync + 'static,
    A: Adapter,
    F: Fn(BuildContext, String, C) -> A + Send + Sync + 'static,
{
    name: &'static str,
    defaults: Option<C>,
    build_fn: F,
    _phantom: PhantomData<fn() -> A>,
}

impl<C, A, F> AdapterFactory<C, A, F>
where
    C: Clone + Send + Sync + 'static,
    A: Adapter,
    F: Fn(BuildContext, String, C) -> A + Send + Sync + 'static,
{
    /// Adapter name (without scope namespace).
    pub fn name(&self) -> &str {
        self.name
    }

    /// Whether a default config was provided.
    pub fn has_defaults(&self) -> bool {
        self.defaults.is_some()
    }

    /// Build an instance using the defaults. Panics if no defaults were
    /// set — use [`Self::with`] instead.
    pub fn call(&self) -> A {
        let cfg = self.defaults.clone().expect(
            "AdapterFactory::call requires `.defaults(...)`; use `.with(config)` otherwise",
        );
        (self.build_fn)(BuildContext, self.name.to_owned(), cfg)
    }

    /// Build an instance using a caller-supplied config.
    pub fn with(&self, config: C) -> A {
        (self.build_fn)(BuildContext, self.name.to_owned(), config)
    }

    /// Build an instance whose `name()` returns `"<base>:<scope>"`. Used
    /// for multi-instance setups (e.g., two Postgres pools — `pg:reads`
    /// and `pg:writes`). Identical to KickJS `.scoped()`.
    pub fn scoped(&self, scope: &str, config: C) -> A {
        let scoped_name = format!("{}:{}", self.name, scope);
        (self.build_fn)(BuildContext, scoped_name, config)
    }
}

/// Begin defining an adapter. See module docs for example.
pub fn define_adapter<C>(name: &'static str) -> AdapterDef<C>
where
    C: Clone + Send + Sync + 'static,
{
    AdapterDef {
        name,
        defaults: None,
    }
}

// ──────────────────────────────── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct PgConfig {
        url: String,
        max_conns: u32,
    }
    impl Default for PgConfig {
        fn default() -> Self {
            Self {
                url: "postgres://localhost".into(),
                max_conns: 10,
            }
        }
    }

    struct PgAdapter {
        name: String,
        cfg: PgConfig,
    }
    #[async_trait]
    impl Adapter for PgAdapter {
        fn name(&self) -> &str {
            &self.name
        }
    }

    /// Closure types are unnameable, so each test builds a fresh factory
    /// inline rather than sharing one via a helper return type.
    fn pg_def() -> AdapterDef<PgConfig> {
        define_adapter::<PgConfig>("postgres").defaults(PgConfig::default())
    }

    #[test]
    fn call_uses_defaults_and_base_name() {
        let f = pg_def().build(|_ctx, name, cfg| PgAdapter { name, cfg });
        let a = f.call();
        assert_eq!(a.name(), "postgres");
        assert_eq!(a.cfg.url, "postgres://localhost");
        assert_eq!(a.cfg.max_conns, 10);
    }

    #[test]
    fn with_overrides_config_keeps_base_name() {
        let f = pg_def().build(|_ctx, name, cfg| PgAdapter { name, cfg });
        let a = f.with(PgConfig {
            url: "postgres://prod".into(),
            max_conns: 50,
        });
        assert_eq!(a.name(), "postgres");
        assert_eq!(a.cfg.url, "postgres://prod");
        assert_eq!(a.cfg.max_conns, 50);
    }

    #[test]
    fn scoped_namespaces_name_and_uses_supplied_config() {
        let f = pg_def().build(|_ctx, name, cfg| PgAdapter { name, cfg });
        let reads = f.scoped("reads", PgConfig::default());
        let writes = f.scoped(
            "writes",
            PgConfig {
                url: "postgres://primary".into(),
                max_conns: 20,
            },
        );

        assert_eq!(reads.name(), "postgres:reads");
        assert_eq!(writes.name(), "postgres:writes");
        assert_eq!(writes.cfg.url, "postgres://primary");
    }

    #[test]
    #[should_panic(expected = ".defaults(...)")]
    fn call_without_defaults_panics() {
        let f =
            define_adapter::<PgConfig>("orphan").build(|_ctx, name, cfg| PgAdapter { name, cfg });
        let _ = f.call();
    }

    #[test]
    fn no_defaults_still_allows_with() {
        let f =
            define_adapter::<PgConfig>("orphan").build(|_ctx, name, cfg| PgAdapter { name, cfg });
        assert!(!f.has_defaults());
        let a = f.with(PgConfig::default());
        assert_eq!(a.name(), "orphan");
    }

    #[tokio::test]
    async fn default_lifecycle_hooks_are_no_ops() {
        let a = pg_def()
            .build(|_ctx, name, cfg| PgAdapter { name, cfg })
            .call();
        let ctx = AdapterContext {
            container: Container::builder().build().unwrap(),
        };
        assert!(a.before_mount(&ctx).await.is_ok());
        assert!(a.before_start(&ctx).await.is_ok());
        assert!(a.after_start(&ctx).await.is_ok());
        assert!(a.shutdown().await.is_ok());
    }
}
