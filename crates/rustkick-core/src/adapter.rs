//! Adapter trait + `define_adapter()` factory.
//!
//! See [`SPEC.md` §4.4](../SPEC.md#44-adapter) for usage and
//! [`ARCHITECTURE.md` §4](../ARCHITECTURE.md#4-adapter-lifecycle--topo-sort)
//! for the lifecycle and topo-sort.

use crate::container::Container;
use crate::error::KickResult;
use async_trait::async_trait;
use std::sync::Arc;

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

    /// Cooperative shutdown — runs in parallel with peer adapters under
    /// `tokio::join!` with a per-adapter timeout budget.
    async fn shutdown(&self) -> KickResult<()> {
        Ok(())
    }
}

/// Context passed to every adapter hook.
pub struct AdapterContext {
    /// Read-side container reference.
    pub container: Container,
}

/// Begin defining an adapter. Returns a typed factory; see
/// [`SPEC.md` §4.9](../SPEC.md#49-factory-variants--scoped-and-async_).
pub fn define_adapter<C, F>(_name: &'static str) -> AdapterFactory<C, F> {
    AdapterFactory {
        _config: std::marker::PhantomData,
        _build: std::marker::PhantomData,
    }
}

/// Opaque factory returned by [`define_adapter`].
pub struct AdapterFactory<C, F> {
    _config: std::marker::PhantomData<fn() -> C>,
    _build: std::marker::PhantomData<F>,
}

// The `.call()`, `.scoped()`, `.async_()` methods land in Phase 1. The
// public surface is reserved here so adopters can write `define_adapter`
// today and not break when the body fills in.

#[allow(dead_code)]
fn _arc_for_lints() -> Option<Arc<()>> {
    None
}
