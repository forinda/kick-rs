//! Context contributor pipeline — typed declarative populators of
//! per-request values.
//!
//! See [`SPEC.md` §4.6](../SPEC.md#46-context-contributor) for the design
//! and [`ARCHITECTURE.md` §5](../ARCHITECTURE.md#5-context-contributor-pipeline)
//! for the runtime contract.
//!
//! ## Mental model
//!
//! Each [`ContextContributor`] declares:
//!
//! - A `Key` type — the value it produces (e.g. `Tenant`, `CurrentUser`)
//! - A `Deps` tuple — values it reads from upstream contributors
//!   (e.g. `(Tenant,)` to depend on a `Tenant` produced earlier)
//! - A `resolve` method — async, returns the produced value
//!
//! ```ignore
//! struct LoadTenant;
//! impl ContextContributor for LoadTenant {
//!     type Key = Tenant;
//!     type Deps = ();
//!     async fn resolve(
//!         &self,
//!         ctx: &dyn ContributorRequest,
//!         _: (),
//!     ) -> KickResult<Tenant> { /* ... */ }
//! }
//!
//! struct LoadProject;
//! impl ContextContributor for LoadProject {
//!     type Key = Project;
//!     type Deps = (Tenant,);
//!     async fn resolve(
//!         &self,
//!         ctx: &dyn ContributorRequest,
//!         (tenant,): (&Tenant,),
//!     ) -> KickResult<Project> { /* ... */ }
//! }
//! ```
//!
//! The pipeline is built at module-composition time, topo-sorted once at
//! boot, and runs sequentially per request. Missing deps fail boot with
//! `RK_E_MISSING_CONTRIBUTOR`; cycles fail with `RK_E_CONTRIBUTOR_CYCLE`.

use crate::container::Container;
use crate::error::{KickError, KickResult};
use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

// ─────────────────────────── Storage interface ───────────────────────────

/// Read-side view of the per-request value store. Contributors and
/// handlers look up upstream-produced values through this trait, plus
/// the per-app [`Container`] when DI resolution is needed inside a
/// contributor's `resolve` body.
pub trait ContributorRequest: Send + Sync {
    /// Lookup a previously-inserted value by its [`TypeId`].
    fn try_get(&self, ty: TypeId) -> Option<&(dyn Any + Send + Sync)>;

    /// Access the app DI container, if the request carries one. Returns
    /// `None` for bare [`ContributorStore::new()`] instances (used in
    /// unit tests). The HTTP middleware always populates this so
    /// production contributors can safely `inject::<T>()` from inside
    /// `resolve()`.
    fn container(&self) -> Option<&Container> {
        None
    }
}

/// Write-side view — the pipeline runner uses this to insert each
/// contributor's output into the store before the next contributor or
/// handler runs.
pub trait MutableContributorRequest: ContributorRequest {
    /// Insert a value under its [`TypeId`].
    fn insert(&mut self, ty: TypeId, value: Box<dyn Any + Send + Sync>);
}

/// Helper: convenience methods for callers who know `T` statically.
pub trait ContributorRequestExt: ContributorRequest {
    /// Typed lookup of an upstream contributor's output — returns
    /// `Some(&T)` if a value of type `T` is in the store. Built on
    /// [`ContributorRequest::try_get`].
    fn get<T: 'static + Send + Sync>(&self) -> Option<&T> {
        self.try_get(TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
    }

    /// Resolve a DI singleton/transient from the request's container.
    /// Panics if no container is attached (only happens in bare
    /// in-memory `ContributorStore` instances used in unit tests).
    fn inject<T: 'static + Send + Sync>(&self) -> Arc<T> {
        self.container()
            .expect(
                "ContributorRequest::inject called without a container — \
                 use ContributorStore::with_container or the HTTP layer",
            )
            .resolve::<T>()
    }

    /// Best-effort resolve — returns `None` if no container is attached
    /// or `T` isn't registered.
    fn try_inject<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        self.container().and_then(|c| c.try_resolve::<T>())
    }
}
impl<T: ContributorRequest + ?Sized> ContributorRequestExt for T {}

/// Default in-memory storage. Used by the HTTP crate to back per-request
/// values and by tests directly.
///
/// Internally each value is wrapped in `Arc` so axum extractors can
/// hand out cheap clones of a contributor's output.
#[derive(Default, Clone)]
pub struct ContributorStore {
    items: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    container: Option<Container>,
}

impl ContributorStore {
    /// Create an empty store with no container attached. Convenient for
    /// unit tests where contributors don't need DI; production paths
    /// should use [`Self::with_container`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty store wired to a DI container. Contributors that
    /// resolve services from inside `resolve()` need this.
    pub fn with_container(container: Container) -> Self {
        Self {
            items: HashMap::new(),
            container: Some(container),
        }
    }

    /// Attach a container to an existing store. Returns the previous
    /// container handle, if any.
    pub fn set_container(&mut self, c: Container) -> Option<Container> {
        self.container.replace(c)
    }

    /// Number of values inserted so far.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the store has any values.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Cloneable handle to a stored value. Returns `None` if no value of
    /// type `T` has been inserted.
    pub fn get_arc<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        let any = self.items.get(&TypeId::of::<T>())?.clone();
        any.downcast::<T>().ok()
    }
}

impl ContributorRequest for ContributorStore {
    fn try_get(&self, ty: TypeId) -> Option<&(dyn Any + Send + Sync)> {
        self.items.get(&ty).map(|a| a.as_ref())
    }
    fn container(&self) -> Option<&Container> {
        self.container.as_ref()
    }
}

impl MutableContributorRequest for ContributorStore {
    fn insert(&mut self, ty: TypeId, value: Box<dyn Any + Send + Sync>) {
        // `Box<T>` -> `Arc<T>` conversion is a no-alloc move into the
        // refcount cell.
        self.items.insert(ty, Arc::from(value));
    }
}

// ───────────────────────────── Deps tuples ────────────────────────────────

/// Trait implemented for tuples of dependency types. Maps a tuple
/// declaration like `(Tenant, User)` to a tuple of resolved references
/// `(&Tenant, &User)` extracted from the per-request store.
///
/// Tuple impls live below for arity 0 through 4 — enough for the typical
/// 0–3 deps without exploding the trait table. Use [`ContributorRequest::try_get`]
/// directly for higher arities.
pub trait ContributorDeps: Send + Sync + 'static {
    /// The tuple-of-references type extracted from the request context.
    type Resolved<'a>: Send;

    /// `TypeId`s of the dependency tuple's elements, in declaration order.
    fn type_ids() -> Vec<TypeId>;

    /// Pull each dep out of `ctx`. Returns `RK_E_MISSING_CONTRIBUTOR`
    /// if any dep is absent — the topo-sort validation runs at boot
    /// so this only fires on framework bugs.
    fn extract(ctx: &dyn ContributorRequest) -> KickResult<Self::Resolved<'_>>;
}

impl ContributorDeps for () {
    type Resolved<'a> = ();
    fn type_ids() -> Vec<TypeId> {
        Vec::new()
    }
    fn extract(_: &dyn ContributorRequest) -> KickResult<()> {
        Ok(())
    }
}

macro_rules! impl_contributor_deps_tuple {
    ( $( $name:ident ),+ ) => {
        impl<$( $name: 'static + Send + Sync, )+> ContributorDeps for ( $( $name, )+ ) {
            type Resolved<'a> = ( $( &'a $name, )+ );

            fn type_ids() -> Vec<TypeId> {
                vec![ $( TypeId::of::<$name>(), )+ ]
            }

            fn extract(ctx: &dyn ContributorRequest) -> KickResult<Self::Resolved<'_>> {
                Ok((
                    $(
                        ctx.try_get(TypeId::of::<$name>())
                            .ok_or_else(|| missing_at_runtime::<$name>())?
                            .downcast_ref::<$name>()
                            .expect("type mismatch — should be unreachable past boot validation"),
                    )+
                ))
            }
        }
    };
}

impl_contributor_deps_tuple!(A);
impl_contributor_deps_tuple!(A, B);
impl_contributor_deps_tuple!(A, B, C);
impl_contributor_deps_tuple!(A, B, C, D);

fn missing_at_runtime<T: 'static>() -> KickError {
    KickError::new(
        "RK_E_MISSING_CONTRIBUTOR",
        format!(
            "required value `{}` missing from request context",
            std::any::type_name::<T>()
        ),
    )
    .with_hint("framework bug — boot-time topo-sort should have caught this")
    .with_context("type", std::any::type_name::<T>())
}

// ──────────────────────────── ContextContributor ─────────────────────────

/// Declarative populator of a per-request value, with typed
/// dependencies that the pipeline topo-sorts at boot.
pub trait ContextContributor: Send + Sync + Sized + 'static {
    /// The value this contributor produces. Stored in the request
    /// context under `TypeId::of::<Self::Key>()`.
    type Key: Send + Sync + 'static;

    /// Tuple of types this contributor reads from upstream
    /// contributors. Use `()` for no dependencies.
    type Deps: ContributorDeps;

    /// Compute the value for the current request.
    ///
    /// The lifetime `'a` ties `&self`, `ctx`, and `deps` (which contains
    /// borrows from the request store) to the same lifetime, captured
    /// in the returned future.
    fn resolve<'a>(
        &'a self,
        ctx: &'a dyn ContributorRequest,
        deps: <Self::Deps as ContributorDeps>::Resolved<'a>,
    ) -> impl Future<Output = KickResult<Self::Key>> + Send + 'a;
}

// ───────────────────── Object-safe erased contributor ─────────────────────

/// Type-erased contributor, used as `Box<dyn ErasedContributor>` inside
/// the pipeline. Bridges the typed [`ContextContributor`] trait so the
/// pipeline can hold a heterogeneous list of contributors.
pub trait ErasedContributor: Send + Sync {
    /// `TypeId` of the value this contributor produces.
    fn produces(&self) -> TypeId;
    /// `TypeId`s of the values this contributor reads.
    fn requires(&self) -> Vec<TypeId>;
    /// Human-readable name of the produced type, for diagnostics.
    fn produces_name(&self) -> &'static str;
    /// Run the contributor against `ctx`, inserting the produced value.
    fn run<'a>(
        &'a self,
        ctx: &'a mut dyn MutableContributorRequest,
    ) -> Pin<Box<dyn Future<Output = KickResult<()>> + Send + 'a>>;
}

/// Alias for type-erased contributors. `Arc` so modules can register a
/// contributor once and have it gathered into the pipeline by reference
/// (modules stay `Clone`-able).
pub type AnyContributor = Arc<dyn ErasedContributor>;

/// Adapter that wraps a typed [`ContextContributor`] into an
/// [`ErasedContributor`].
struct ContributorAdapter<C: ContextContributor> {
    inner: C,
}

impl<C: ContextContributor> ErasedContributor for ContributorAdapter<C> {
    fn produces(&self) -> TypeId {
        TypeId::of::<C::Key>()
    }
    fn requires(&self) -> Vec<TypeId> {
        C::Deps::type_ids()
    }
    fn produces_name(&self) -> &'static str {
        std::any::type_name::<C::Key>()
    }
    fn run<'a>(
        &'a self,
        ctx: &'a mut dyn MutableContributorRequest,
    ) -> Pin<Box<dyn Future<Output = KickResult<()>> + Send + 'a>> {
        Box::pin(async move {
            // Two immutable borrows of `*ctx` coexist while resolve is in
            // flight (one for deps refs, one for the `ctx` parameter);
            // the mutable insert happens only after both are dropped.
            let deps = <C::Deps as ContributorDeps>::extract(&*ctx)?;
            let key = self.inner.resolve(&*ctx, deps).await?;
            ctx.insert(TypeId::of::<C::Key>(), Box::new(key));
            Ok(())
        })
    }
}

/// Wrap a typed contributor as an [`AnyContributor`] for storage in a
/// module / pipeline.
pub fn erase<C: ContextContributor>(c: C) -> AnyContributor {
    Arc::new(ContributorAdapter { inner: c })
}

// ───────────────────────────── Pipeline ───────────────────────────────────

/// A built pipeline — topo-sorted contributors ready to run sequentially
/// against a request context. Construct via [`ContributorPipeline::build`].
pub struct ContributorPipeline {
    sorted: Vec<AnyContributor>,
}

impl std::fmt::Debug for ContributorPipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContributorPipeline")
            .field("order", &self.order())
            .finish()
    }
}

impl ContributorPipeline {
    /// Topo-sort `items` by their `produces → requires` graph and
    /// validate that every required key is produced by some contributor
    /// in the set.
    ///
    /// Errors:
    /// - `RK_E_MISSING_CONTRIBUTOR` — a contributor requires a type no
    ///   other contributor produces.
    /// - `RK_E_CONTRIBUTOR_CYCLE` — the dep graph contains a cycle.
    /// - `RK_E_DUPLICATE_CONTRIBUTOR` — two contributors produce the
    ///   same `Key` type.
    pub fn build(items: Vec<AnyContributor>) -> KickResult<Self> {
        // Index produced TypeId -> position in items.
        let mut produced_by: HashMap<TypeId, usize> = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            let key = item.produces();
            if let Some(prev) = produced_by.insert(key, i) {
                return Err(KickError::new(
                    "RK_E_DUPLICATE_CONTRIBUTOR",
                    format!(
                        "two contributors produce `{}`: positions {} and {}",
                        item.produces_name(),
                        prev,
                        i
                    ),
                )
                .with_hint("rename one of the `Key` types or remove the duplicate registration"));
            }
        }

        // Build edge list: producer -> consumer.
        let n = items.len();
        let mut in_degree: Vec<usize> = vec![0; n];
        let mut out_edges: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (consumer_idx, item) in items.iter().enumerate() {
            for dep_ty in item.requires() {
                let Some(&producer_idx) = produced_by.get(&dep_ty) else {
                    return Err(KickError::new(
                        "RK_E_MISSING_CONTRIBUTOR",
                        format!(
                            "contributor producing `{}` requires `{:?}` but nothing produces it",
                            item.produces_name(),
                            dep_ty
                        ),
                    )
                    .with_hint(
                        "add a contributor that produces this type, or remove the dependency",
                    ));
                };
                if producer_idx == consumer_idx {
                    return Err(KickError::new(
                        "RK_E_CONTRIBUTOR_CYCLE",
                        format!(
                            "contributor producing `{}` lists its own output as a dep",
                            item.produces_name()
                        ),
                    ));
                }
                out_edges[producer_idx].push(consumer_idx);
                in_degree[consumer_idx] += 1;
            }
        }

        // Kahn.
        let mut ready: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order: Vec<usize> = Vec::with_capacity(n);
        while let Some(i) = ready.pop_front() {
            order.push(i);
            // Cloned to release the immutable borrow before mutating in_degree.
            let outs = out_edges[i].clone();
            for j in outs {
                in_degree[j] -= 1;
                if in_degree[j] == 0 {
                    ready.push_back(j);
                }
            }
        }
        if order.len() != n {
            let stuck: HashSet<usize> = (0..n).filter(|i| !order.contains(i)).collect();
            let names: Vec<_> = stuck.iter().map(|&i| items[i].produces_name()).collect();
            return Err(KickError::new(
                "RK_E_CONTRIBUTOR_CYCLE",
                format!("cycle detected in contributor graph; involved: {names:?}"),
            )
            .with_hint("break the cycle in `type Deps = (...)` declarations"));
        }

        // Reorder items by computed sort order.
        let pos: HashMap<usize, usize> = order
            .iter()
            .enumerate()
            .map(|(rank, idx)| (*idx, rank))
            .collect();
        let mut indexed: Vec<(usize, AnyContributor)> = items.into_iter().enumerate().collect();
        indexed.sort_by_key(|(orig, _)| pos[orig]);
        let sorted = indexed.into_iter().map(|(_, c)| c).collect();

        Ok(Self { sorted })
    }

    /// Number of contributors in the pipeline.
    pub fn len(&self) -> usize {
        self.sorted.len()
    }

    /// Whether the pipeline has any contributors.
    pub fn is_empty(&self) -> bool {
        self.sorted.is_empty()
    }

    /// Names of produced types in execution order, for diagnostics
    /// (e.g., the `/__debug` endpoint or `cargo kick check`).
    pub fn order(&self) -> Vec<&'static str> {
        self.sorted.iter().map(|c| c.produces_name()).collect()
    }

    /// Run every contributor in topo order against `ctx`.
    pub async fn run(&self, ctx: &mut dyn MutableContributorRequest) -> KickResult<()> {
        for c in &self.sorted {
            c.run(ctx).await?;
        }
        Ok(())
    }
}

// ──────────────────────────────── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test scaffolding ──────────────────────────────────────────────────

    #[derive(Debug, PartialEq)]
    struct Tenant {
        id: u32,
    }
    #[derive(Debug, PartialEq)]
    struct Project {
        tenant_id: u32,
        name: &'static str,
    }
    #[derive(Debug, PartialEq)]
    struct Doc {
        project: &'static str,
        path: &'static str,
    }

    struct LoadTenant;
    impl ContextContributor for LoadTenant {
        type Key = Tenant;
        type Deps = ();
        async fn resolve<'a>(
            &'a self,
            _ctx: &'a dyn ContributorRequest,
            _: (),
        ) -> KickResult<Tenant> {
            Ok(Tenant { id: 42 })
        }
    }

    struct LoadProject;
    impl ContextContributor for LoadProject {
        type Key = Project;
        type Deps = (Tenant,);
        async fn resolve<'a>(
            &'a self,
            _ctx: &'a dyn ContributorRequest,
            (tenant,): (&'a Tenant,),
        ) -> KickResult<Project> {
            Ok(Project {
                tenant_id: tenant.id,
                name: "kick-rs",
            })
        }
    }

    struct LoadDoc;
    impl ContextContributor for LoadDoc {
        type Key = Doc;
        type Deps = (Project,);
        async fn resolve<'a>(
            &'a self,
            _ctx: &'a dyn ContributorRequest,
            (project,): (&'a Project,),
        ) -> KickResult<Doc> {
            Ok(Doc {
                project: project.name,
                path: "/spec",
            })
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn empty_pipeline_runs() {
        let p = ContributorPipeline::build(vec![]).unwrap();
        let mut store = ContributorStore::new();
        p.run(&mut store).await.unwrap();
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn single_contributor_inserts_value() {
        let p = ContributorPipeline::build(vec![erase(LoadTenant)]).unwrap();
        let mut store = ContributorStore::new();
        p.run(&mut store).await.unwrap();
        assert_eq!(store.get::<Tenant>(), Some(&Tenant { id: 42 }));
    }

    #[tokio::test]
    async fn chain_orders_by_deps() {
        // Insert OUT OF ORDER — topo-sort must put Tenant first.
        let p =
            ContributorPipeline::build(vec![erase(LoadDoc), erase(LoadProject), erase(LoadTenant)])
                .unwrap();

        let order = p.order();
        assert_eq!(order.first().unwrap(), &std::any::type_name::<Tenant>());
        assert_eq!(order.last().unwrap(), &std::any::type_name::<Doc>());

        let mut store = ContributorStore::new();
        p.run(&mut store).await.unwrap();
        assert_eq!(
            store.get::<Doc>(),
            Some(&Doc {
                project: "kick-rs",
                path: "/spec"
            })
        );
    }

    #[test]
    fn missing_dep_fails_build() {
        // LoadProject requires Tenant, which we don't register.
        let err = ContributorPipeline::build(vec![erase(LoadProject)]).unwrap_err();
        assert_eq!(err.code, "RK_E_MISSING_CONTRIBUTOR");
    }

    #[test]
    fn duplicate_producer_fails_build() {
        let err =
            ContributorPipeline::build(vec![erase(LoadTenant), erase(LoadTenant)]).unwrap_err();
        assert_eq!(err.code, "RK_E_DUPLICATE_CONTRIBUTOR");
    }

    struct SelfDep;
    impl ContextContributor for SelfDep {
        type Key = u32;
        type Deps = (u32,);
        async fn resolve<'a>(
            &'a self,
            _: &'a dyn ContributorRequest,
            (n,): (&'a u32,),
        ) -> KickResult<u32> {
            Ok(*n + 1)
        }
    }

    #[test]
    fn self_dep_is_a_cycle_error() {
        let err = ContributorPipeline::build(vec![erase(SelfDep)]).unwrap_err();
        assert_eq!(err.code, "RK_E_CONTRIBUTOR_CYCLE");
    }

    #[derive(Debug)]
    struct Left;
    #[derive(Debug)]
    struct Right;

    struct WantsRight;
    impl ContextContributor for WantsRight {
        type Key = Left;
        type Deps = (Right,);
        async fn resolve<'a>(
            &'a self,
            _: &'a dyn ContributorRequest,
            _: (&'a Right,),
        ) -> KickResult<Left> {
            Ok(Left)
        }
    }

    struct WantsLeft;
    impl ContextContributor for WantsLeft {
        type Key = Right;
        type Deps = (Left,);
        async fn resolve<'a>(
            &'a self,
            _: &'a dyn ContributorRequest,
            _: (&'a Left,),
        ) -> KickResult<Right> {
            Ok(Right)
        }
    }

    #[test]
    fn two_node_cycle_detected() {
        let err =
            ContributorPipeline::build(vec![erase(WantsRight), erase(WantsLeft)]).unwrap_err();
        assert_eq!(err.code, "RK_E_CONTRIBUTOR_CYCLE");
    }

    // ── Container access via ContributorRequest::container() ────────────────

    struct ReadTenant {
        tenant_id: u32,
    }

    struct LoadTenantFromDi;
    impl ContextContributor for LoadTenantFromDi {
        type Key = Tenant;
        type Deps = ();
        async fn resolve<'a>(
            &'a self,
            ctx: &'a dyn ContributorRequest,
            _: (),
        ) -> KickResult<Tenant> {
            // DI-resolve the tenant id from a singleton bound by the
            // user (typical case: an auth header is parsed elsewhere
            // and the result registered as a request-scoped singleton).
            // For this unit test we register at boot.
            let cfg = ctx.inject::<ReadTenant>();
            Ok(Tenant { id: cfg.tenant_id })
        }
    }

    #[tokio::test]
    async fn contributor_can_inject_from_container() {
        use crate::Container;

        let container = Container::builder()
            .singleton(ReadTenant { tenant_id: 99 })
            .build()
            .unwrap();

        let p = ContributorPipeline::build(vec![erase(LoadTenantFromDi)]).unwrap();
        let mut store = ContributorStore::with_container(container);
        p.run(&mut store).await.unwrap();

        assert_eq!(store.get::<Tenant>(), Some(&Tenant { id: 99 }));
    }

    #[tokio::test]
    async fn try_inject_handles_missing_container_gracefully() {
        // Bare ContributorStore::new() has no container — try_inject
        // returns None instead of panicking.
        let store = ContributorStore::new();
        assert!(store.try_inject::<ReadTenant>().is_none());
    }

    #[tokio::test]
    async fn store_lookup_is_repeatable() {
        let p = ContributorPipeline::build(vec![erase(LoadTenant)]).unwrap();
        let mut store = ContributorStore::new();
        p.run(&mut store).await.unwrap();

        let a = store.get::<Tenant>().unwrap();
        let b = store.get::<Tenant>().unwrap();
        assert!(
            std::ptr::eq(a, b),
            "ctx.get::<T>() must return the same reference each call"
        );
    }
}
