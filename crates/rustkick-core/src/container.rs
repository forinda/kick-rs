//! DI container — typed providers, three scopes, structured boot errors.
//!
//! See [`ARCHITECTURE.md` §1](../ARCHITECTURE.md#1-container-internals) for
//! the storage layout and resolution path. This skeleton exposes the public
//! API only; concrete implementation lands in the next phase.

use crate::error::{KickError, KickResult};
use std::any::Any;
use std::sync::Arc;

/// Type alias for storage of any singleton.
type AnyArc = Arc<dyn Any + Send + Sync>;

/// Read-side container handle. Cheap to clone.
#[derive(Clone)]
pub struct Container {
    _inner: Arc<ContainerInner>,
}

#[allow(dead_code)]
struct ContainerInner {
    // Real fields land in the implementation phase. Kept private so the
    // public API can stabilize independently of the storage strategy.
}

impl Container {
    /// Begin building a new container.
    pub fn builder() -> ContainerBuilder {
        ContainerBuilder::default()
    }

    /// Resolve a singleton/transient instance by type.
    ///
    /// Panics if the builder validation was bypassed and the type isn't
    /// registered — this signals a framework bug, never user error.
    pub fn resolve<T: 'static + Send + Sync>(&self) -> Arc<T> {
        todo!("Container::resolve — implementation pending Phase 1")
    }

    /// Best-effort lookup; returns `None` if the type isn't registered.
    pub fn try_resolve<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        todo!("Container::try_resolve — implementation pending Phase 1")
    }
}

/// Builder collecting provider specs before validation + instantiation.
#[derive(Default)]
pub struct ContainerBuilder {
    // Provider specs accumulate here; validation runs at `.build()`.
}

impl ContainerBuilder {
    /// Register a singleton instance by type.
    pub fn singleton<T: 'static + Send + Sync>(self, _instance: T) -> Self {
        // todo: store in provider list
        self
    }

    /// Register a singleton built by a factory closure.
    pub fn singleton_factory<T, F>(self, _factory: F) -> Self
    where
        T: 'static + Send + Sync,
        F: Fn(&Container) -> Arc<T> + Send + Sync + 'static,
    {
        self
    }

    /// Finalize the container. Validates the DI graph and instantiates
    /// singletons in topological order.
    pub fn build(self) -> KickResult<Container> {
        Err(KickError::new(
            "RK_E_UNIMPLEMENTED",
            "ContainerBuilder::build is not yet implemented",
        )
        .with_hint("Implementation lands in Phase 1 of the roadmap"))
    }
}

// `_` to satisfy unused warnings on the singleton/factory helpers above.
#[allow(dead_code)]
fn _used_to_silence_warnings(_: AnyArc) {}
