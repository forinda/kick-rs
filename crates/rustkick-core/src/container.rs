//! DI container — typed providers with three scopes (singleton, transient,
//! request) backed by a `TypeId` registry.
//!
//! See [`ARCHITECTURE.md` §1](../ARCHITECTURE.md#1-container-internals) for
//! storage layout and resolution path.
//!
//! ## What's implemented in Phase 1.1
//!
//! - `singleton::<T>(value)` — direct value binding
//! - `singleton_factory::<T>(|c| …)` — lazy factory, evaluated on first
//!   resolve and cached for the container's lifetime
//! - `transient::<T>(|c| …)` — factory called on every resolve
//! - Build-time duplicate detection (`RK_E_AMBIGUOUS_BIND`)
//! - `resolve` / `try_resolve` / `contains`
//!
//! ## What's deferred
//!
//! - Cycle / missing-dep / scope-violation validation at build time —
//!   requires explicit dependency declarations (Phase 3 macros).
//! - Request-scoped providers wire into a transport-level extension map;
//!   the HTTP crate adds that integration in Phase 1.4.
//! - Named [`Token<T>`](crate::Token)-keyed bindings — Phase 2.

use crate::error::{KickError, KickResult};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Erased Arc — what the storage actually holds.
type AnyArc = Arc<dyn Any + Send + Sync>;

/// Factory closure for both singleton-factory and transient providers.
type Factory = Arc<dyn Fn(&Container) -> AnyArc + Send + Sync>;

/// Read-side container handle. Cheap to `clone()` (refcount bump only).
#[derive(Clone)]
pub struct Container {
    inner: Arc<ContainerInner>,
}

impl std::fmt::Debug for Container {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Container")
            .field("registered_types", &self.inner.type_names.values().collect::<Vec<_>>())
            .finish()
    }
}

struct ContainerInner {
    /// Concrete singleton values. Populated either at build time (from
    /// `singleton(v)`) or lazily on first `resolve` (from
    /// `singleton_factory(f)`).
    singletons: RwLock<HashMap<TypeId, AnyArc>>,

    /// Factories that *produce* singletons. Drained into [`Self::singletons`]
    /// on first resolve. Stored without locking — read-only after build.
    singleton_factories: HashMap<TypeId, Factory>,

    /// Factories that run on every resolve. No caching.
    transient_factories: HashMap<TypeId, Factory>,

    /// Type names for diagnostics. Keyed by `TypeId` to keep the hot
    /// path cheap (`TypeId` is `Copy + Hash`).
    type_names: HashMap<TypeId, &'static str>,
}

impl Container {
    /// Begin building a container.
    pub fn builder() -> ContainerBuilder {
        ContainerBuilder::default()
    }

    /// Resolve a singleton or transient by type.
    ///
    /// # Panics
    ///
    /// Panics if `T` was never registered. Use [`Self::try_resolve`] for
    /// the fallible variant.
    pub fn resolve<T: 'static + Send + Sync>(&self) -> Arc<T> {
        self.try_resolve::<T>().unwrap_or_else(|| {
            panic!(
                "Container::resolve called for unregistered type `{}` — \
                 register it with .singleton/.singleton_factory/.transient before .build()",
                std::any::type_name::<T>()
            )
        })
    }

    /// Best-effort lookup. Returns `None` if no provider matches.
    pub fn try_resolve<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        let key = TypeId::of::<T>();

        // 1. Singleton (already-built) — read lock, fast path.
        if let Some(arc) = self.inner.singletons.read().ok()?.get(&key).cloned() {
            return arc.downcast::<T>().ok();
        }

        // 2. Singleton factory — evaluate once, cache, return.
        if let Some(factory) = self.inner.singleton_factories.get(&key) {
            let produced = factory(self);
            let mut singletons = self.inner.singletons.write().ok()?;
            // Double-checked: another thread may have populated meanwhile.
            let entry = singletons.entry(key).or_insert(produced);
            return Arc::clone(entry).downcast::<T>().ok();
        }

        // 3. Transient factory — evaluate every time, no cache.
        if let Some(factory) = self.inner.transient_factories.get(&key) {
            let produced = factory(self);
            return produced.downcast::<T>().ok();
        }

        None
    }

    /// Whether a provider for `T` is registered.
    pub fn contains<T: 'static + Send + Sync>(&self) -> bool {
        let key = TypeId::of::<T>();
        self.inner.singletons.read().is_ok_and(|m| m.contains_key(&key))
            || self.inner.singleton_factories.contains_key(&key)
            || self.inner.transient_factories.contains_key(&key)
    }
}

// ───────────────────────────────── Builder ─────────────────────────────────

/// Builder used to assemble a [`Container`].
#[derive(Default)]
pub struct ContainerBuilder {
    singletons: HashMap<TypeId, AnyArc>,
    singleton_factories: HashMap<TypeId, Factory>,
    transient_factories: HashMap<TypeId, Factory>,
    type_names: HashMap<TypeId, &'static str>,
    /// Errors accumulated during registration. Surfaced at [`Self::build`].
    errors: Vec<KickError>,
}

impl ContainerBuilder {
    /// Register `value` as a singleton.
    pub fn singleton<T: 'static + Send + Sync>(self, value: T) -> Self {
        self.singleton_arc(Arc::new(value))
    }

    /// Register an already-wrapped singleton. Useful when the same Arc
    /// must be shared between multiple container builds (e.g., a module
    /// being folded into multiple `register_into` calls).
    pub fn singleton_arc<T: 'static + Send + Sync>(mut self, value: Arc<T>) -> Self {
        let key = TypeId::of::<T>();
        if self.is_registered(key) {
            self.errors.push(ambiguous_bind::<T>());
            return self;
        }
        self.type_names.insert(key, std::any::type_name::<T>());
        self.singletons.insert(key, value);
        self
    }

    /// Register a singleton built lazily by `factory`. The factory receives
    /// the container so it can resolve sibling providers.
    ///
    /// Called at most once for the lifetime of the resulting container.
    pub fn singleton_factory<T, F>(mut self, factory: F) -> Self
    where
        T: 'static + Send + Sync,
        F: Fn(&Container) -> Arc<T> + Send + Sync + 'static,
    {
        let key = TypeId::of::<T>();
        if self.is_registered(key) {
            self.errors.push(ambiguous_bind::<T>());
            return self;
        }
        self.type_names.insert(key, std::any::type_name::<T>());
        let erased: Factory = Arc::new(move |c| {
            let typed = factory(c);
            typed as AnyArc
        });
        self.singleton_factories.insert(key, erased);
        self
    }

    /// Register `factory` as a transient — fires on every `resolve::<T>()`.
    pub fn transient<T, F>(mut self, factory: F) -> Self
    where
        T: 'static + Send + Sync,
        F: Fn(&Container) -> T + Send + Sync + 'static,
    {
        let key = TypeId::of::<T>();
        if self.is_registered(key) {
            self.errors.push(ambiguous_bind::<T>());
            return self;
        }
        self.type_names.insert(key, std::any::type_name::<T>());
        let erased: Factory = Arc::new(move |c| {
            let value = factory(c);
            Arc::new(value) as AnyArc
        });
        self.transient_factories.insert(key, erased);
        self
    }

    /// Finalize. Returns the first registration error, if any.
    pub fn build(self) -> KickResult<Container> {
        if let Some(first) = self.errors.into_iter().next() {
            return Err(first);
        }
        Ok(Container {
            inner: Arc::new(ContainerInner {
                singletons: RwLock::new(self.singletons),
                singleton_factories: self.singleton_factories,
                transient_factories: self.transient_factories,
                type_names: self.type_names,
            }),
        })
    }

    fn is_registered(&self, key: TypeId) -> bool {
        self.singletons.contains_key(&key)
            || self.singleton_factories.contains_key(&key)
            || self.transient_factories.contains_key(&key)
    }
}

fn ambiguous_bind<T: 'static>() -> KickError {
    KickError::new(
        "RK_E_AMBIGUOUS_BIND",
        format!("two providers registered for `{}`", std::any::type_name::<T>()),
    )
    .with_hint("each scope+type pair must be unique; use `Token<T>` for a second binding")
    .with_context("type", std::any::type_name::<T>())
}

// ──────────────────────────────── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, PartialEq)]
    struct Greeter(String);

    #[test]
    fn singleton_resolves_to_registered_value() {
        let c = Container::builder()
            .singleton(Greeter("hello".into()))
            .build()
            .unwrap();
        let g = c.resolve::<Greeter>();
        assert_eq!(g.0, "hello");
    }

    #[test]
    fn singleton_resolve_returns_same_arc() {
        let c = Container::builder()
            .singleton(Greeter("once".into()))
            .build()
            .unwrap();
        let a = c.resolve::<Greeter>();
        let b = c.resolve::<Greeter>();
        assert!(Arc::ptr_eq(&a, &b), "singleton must return the same Arc");
    }

    #[test]
    fn singleton_factory_runs_once_and_caches() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        CALLS.store(0, Ordering::SeqCst);

        let c = Container::builder()
            .singleton_factory::<Greeter, _>(|_| {
                CALLS.fetch_add(1, Ordering::SeqCst);
                Arc::new(Greeter("from-factory".into()))
            })
            .build()
            .unwrap();

        let _ = c.resolve::<Greeter>();
        let _ = c.resolve::<Greeter>();
        let _ = c.resolve::<Greeter>();
        assert_eq!(CALLS.load(Ordering::SeqCst), 1, "singleton factory must run exactly once");
    }

    #[test]
    fn singleton_factory_can_resolve_other_providers() {
        struct Db(String);
        struct Repo {
            db_url: String,
        }
        let c = Container::builder()
            .singleton(Db("postgres://localhost".into()))
            .singleton_factory::<Repo, _>(|c| {
                let db = c.resolve::<Db>();
                Arc::new(Repo { db_url: db.0.clone() })
            })
            .build()
            .unwrap();
        let repo = c.resolve::<Repo>();
        assert_eq!(repo.db_url, "postgres://localhost");
    }

    #[test]
    fn transient_fires_per_resolve() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        COUNTER.store(0, Ordering::SeqCst);

        #[derive(Debug, PartialEq)]
        struct Ticket(usize);

        let c = Container::builder()
            .transient::<Ticket, _>(|_| Ticket(COUNTER.fetch_add(1, Ordering::SeqCst)))
            .build()
            .unwrap();

        let a = c.resolve::<Ticket>();
        let b = c.resolve::<Ticket>();
        assert_ne!(a.0, b.0, "transients must produce a fresh value each resolve");
        assert!(!Arc::ptr_eq(&a, &b), "transients must not share Arc storage");
    }

    #[test]
    fn try_resolve_returns_none_for_unregistered() {
        let c = Container::builder().build().unwrap();
        assert!(c.try_resolve::<Greeter>().is_none());
    }

    #[test]
    fn contains_reports_registration() {
        let c = Container::builder()
            .singleton(Greeter("x".into()))
            .build()
            .unwrap();
        assert!(c.contains::<Greeter>());
        assert!(!c.contains::<u32>());
    }

    #[test]
    fn duplicate_singleton_is_ambiguous_bind() {
        let err = Container::builder()
            .singleton(Greeter("a".into()))
            .singleton(Greeter("b".into()))
            .build()
            .unwrap_err();
        assert_eq!(err.code, "RK_E_AMBIGUOUS_BIND");
        assert!(err.message.contains("Greeter"));
    }

    #[test]
    fn singleton_then_factory_is_ambiguous_bind() {
        let err = Container::builder()
            .singleton(Greeter("v".into()))
            .singleton_factory::<Greeter, _>(|_| Arc::new(Greeter("f".into())))
            .build()
            .unwrap_err();
        assert_eq!(err.code, "RK_E_AMBIGUOUS_BIND");
    }

    #[test]
    fn singleton_and_transient_for_same_type_is_ambiguous_bind() {
        let err = Container::builder()
            .singleton(Greeter("v".into()))
            .transient::<Greeter, _>(|_| Greeter("t".into()))
            .build()
            .unwrap_err();
        assert_eq!(err.code, "RK_E_AMBIGUOUS_BIND");
    }

    #[test]
    fn empty_builder_builds() {
        let c = Container::builder().build().unwrap();
        assert!(!c.contains::<Greeter>());
    }

    #[test]
    #[should_panic(expected = "unregistered type")]
    fn resolve_panics_on_unregistered() {
        let c = Container::builder().build().unwrap();
        let _ = c.resolve::<Greeter>();
    }
}
