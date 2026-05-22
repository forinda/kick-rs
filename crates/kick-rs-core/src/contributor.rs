//! Context contributor pipeline — see [`SPEC.md` §4.6](../SPEC.md#46-context-contributor)
//! and [`ARCHITECTURE.md` §5](../ARCHITECTURE.md#5-context-contributor-pipeline).

use crate::error::KickResult;
use async_trait::async_trait;

/// Declarative populator of a per-request value (`ctx.get::<Self::Key>()`),
/// with typed dependencies that the topo-sort verifies at boot.
#[async_trait]
pub trait ContextContributor: Send + Sync + 'static {
    /// Type produced by [`Self::resolve`] and stored on the request.
    type Key: Send + Sync + 'static;

    /// Tuple of values produced by upstream contributors that this one
    /// reads. Use `()` for no dependencies.
    type Deps: Send + Sync + 'static;

    /// Compute the value for the current request.
    async fn resolve(
        &self,
        // `&dyn` here so this trait stays HTTP-agnostic; concrete
        // `RequestContext` lives in `kick-rs-http`.
        ctx: &dyn ContributorRequest,
        deps: Self::Deps,
    ) -> KickResult<Self::Key>;
}

/// Type-erased contributor for storage on a [`Module`](crate::Module).
pub trait AnyContributor: Send + Sync {
    /// Returns the `TypeId` of the produced key.
    fn produces(&self) -> std::any::TypeId;
    /// Returns the `TypeId` of each required dep, in declaration order.
    fn requires(&self) -> Vec<std::any::TypeId>;
}

/// Transport-agnostic view of a request — `kick-rs-http` implements this
/// for `RequestContext`. Enables `kick-rs-core` to stay free of axum
/// types so workers / CLIs can use the same contributor pipeline later.
pub trait ContributorRequest: Send + Sync {
    /// Lookup a previously-produced value by type.
    fn try_get(&self, ty: std::any::TypeId) -> Option<&(dyn std::any::Any + Send + Sync)>;
}
