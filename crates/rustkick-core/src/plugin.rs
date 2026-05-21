//! Plugin trait + `define_plugin()` factory.
//!
//! Plugins are like adapters minus the long-lived lifecycle — pure DI
//! bindings + tower layers + contributors. See [`SPEC.md` §5](../SPEC.md#5-plugins-deep-dive).

use crate::container::ContainerBuilder;
use crate::contributor::AnyContributor;
use crate::error::KickResult;

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
    fn contributors(&self) -> Vec<Box<dyn AnyContributor>> {
        Vec::new()
    }
}

/// Begin defining a plugin. Returns a typed factory.
pub fn define_plugin<C>(_name: &'static str) -> PluginFactory<C> {
    PluginFactory {
        _config: std::marker::PhantomData,
    }
}

/// Opaque factory returned by [`define_plugin`].
pub struct PluginFactory<C> {
    _config: std::marker::PhantomData<fn() -> C>,
}
