//! Module composition — see [`SPEC.md` §4.3](../SPEC.md#43-module).
//!
//! A [`Module`] is a transport-agnostic bundle of DI providers + sub-modules
//! identified by a stable name and a prefix. HTTP-specific concerns
//! (routes, handlers) live in `rustkick-http` and wrap `Module`.
//!
//! ## What's implemented in Phase 1.2
//!
//! - `ModuleBuilder` with `service_value`, `service_factory`, `transient`,
//!   `sub_module`, `prefix`
//! - [`Module::register_into`] folds all providers (recursively across
//!   sub-modules) into a [`ContainerBuilder`]
//!
//! ## Deferred
//!
//! - Auto-wired `.service::<T>()` lands when `#[service]` macro arrives
//!   (Phase 3) — until then use `service_value` / `service_factory`.
//! - Route storage lives in the HTTP crate (Phase 1.4).
//! - Contributor attachment (`.contribute::<C>()`) lands in Phase 4.

use crate::container::{Container, ContainerBuilder};
use std::sync::Arc;

/// Registration callback — type-erased so providers of any `T` can sit
/// in a single `Vec`. Each spec records its target type's name for the
/// duplicate-detection error message produced by [`ContainerBuilder`].
type RegisterFn = Arc<dyn Fn(ContainerBuilder) -> ContainerBuilder + Send + Sync>;

/// One DI provider declared by a module.
#[derive(Clone)]
pub struct ProviderSpec {
    type_name: &'static str,
    register: RegisterFn,
}

impl std::fmt::Debug for ProviderSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderSpec")
            .field("type", &self.type_name)
            .finish()
    }
}

/// Built module — transport-agnostic. HTTP wiring (routes, handlers) lives
/// in `rustkick-http` and wraps this type.
#[derive(Debug, Clone)]
pub struct Module {
    /// Stable name used for logging, introspection, and `depends_on`.
    pub name: String,
    /// URL/path prefix applied by HTTP wrappers; semantically opaque here.
    pub prefix: String,
    providers: Vec<ProviderSpec>,
    sub_modules: Vec<Module>,
}

impl Module {
    /// Name of every provider this module (and its sub-modules) declares.
    /// Mostly useful for diagnostics and introspection.
    pub fn provider_type_names(&self) -> Vec<&'static str> {
        let mut out: Vec<_> = self.providers.iter().map(|p| p.type_name).collect();
        for sub in &self.sub_modules {
            out.extend(sub.provider_type_names());
        }
        out
    }

    /// Number of direct + transitive providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
            + self
                .sub_modules
                .iter()
                .map(|s| s.provider_count())
                .sum::<usize>()
    }

    /// Fold every provider in this module (and recursively, every
    /// sub-module) into `builder`. Duplicate-type collisions across modules
    /// surface at `ContainerBuilder::build()` as `RK_E_AMBIGUOUS_BIND`.
    pub fn register_into(&self, mut builder: ContainerBuilder) -> ContainerBuilder {
        for p in &self.providers {
            builder = (p.register)(builder);
        }
        for sub in &self.sub_modules {
            builder = sub.register_into(builder);
        }
        builder
    }

    /// Direct sub-modules (read-only). Used by HTTP wrappers to compose
    /// nested routers; not needed in typical app code.
    pub fn sub_modules(&self) -> &[Module] {
        &self.sub_modules
    }
}

// ───────────────────────────────── Builder ─────────────────────────────────

/// Fluent builder for [`Module`].
pub struct ModuleBuilder {
    name: String,
    prefix: String,
    providers: Vec<ProviderSpec>,
    sub_modules: Vec<Module>,
}

/// Entry point — start composing a module.
pub fn define_module(name: impl Into<String>) -> ModuleBuilder {
    ModuleBuilder {
        name: name.into(),
        prefix: String::new(),
        providers: Vec::new(),
        sub_modules: Vec::new(),
    }
}

impl ModuleBuilder {
    /// URL/path prefix applied to every route in this module. Sub-modules
    /// concatenate after the parent prefix when an HTTP wrapper composes them.
    pub fn prefix(mut self, p: impl Into<String>) -> Self {
        self.prefix = p.into();
        self
    }

    /// Bind a pre-built singleton.
    pub fn service_value<T: 'static + Send + Sync>(mut self, value: T) -> Self {
        let value = Arc::new(value);
        let cloned = Arc::clone(&value);
        let register: RegisterFn = Arc::new(move |b: ContainerBuilder| {
            // Move a fresh Arc clone in each invocation — register_into
            // may be called multiple times across builds in tests.
            b.singleton_arc::<T>(Arc::clone(&cloned))
        });
        self.providers.push(ProviderSpec {
            type_name: std::any::type_name::<T>(),
            register,
        });
        self
    }

    /// Bind a lazily-constructed singleton.
    pub fn service_factory<T, F>(mut self, factory: F) -> Self
    where
        T: 'static + Send + Sync,
        F: Fn(&Container) -> Arc<T> + Send + Sync + 'static,
    {
        let factory = Arc::new(factory);
        let cloned = Arc::clone(&factory);
        let register: RegisterFn = Arc::new(move |b: ContainerBuilder| {
            let f = Arc::clone(&cloned);
            b.singleton_factory::<T, _>(move |c| (f)(c))
        });
        self.providers.push(ProviderSpec {
            type_name: std::any::type_name::<T>(),
            register,
        });
        self
    }

    /// Bind a transient provider — runs per-resolve, never cached.
    pub fn transient<T, F>(mut self, factory: F) -> Self
    where
        T: 'static + Send + Sync,
        F: Fn(&Container) -> T + Send + Sync + 'static,
    {
        let factory = Arc::new(factory);
        let cloned = Arc::clone(&factory);
        let register: RegisterFn = Arc::new(move |b: ContainerBuilder| {
            let f = Arc::clone(&cloned);
            b.transient::<T, _>(move |c| (f)(c))
        });
        self.providers.push(ProviderSpec {
            type_name: std::any::type_name::<T>(),
            register,
        });
        self
    }

    /// Mount a sub-module under this module. Sub-module prefixes are
    /// concatenated by HTTP wrappers (`parent.prefix` + `child.prefix`).
    pub fn sub_module(mut self, m: Module) -> Self {
        self.sub_modules.push(m);
        self
    }

    /// Finalize the module.
    pub fn build(self) -> Module {
        Module {
            name: self.name,
            prefix: self.prefix,
            providers: self.providers,
            sub_modules: self.sub_modules,
        }
    }
}

// ──────────────────────────────── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Greeter(String);

    #[derive(Debug)]
    struct Counter(#[allow(dead_code)] std::sync::atomic::AtomicUsize);

    #[test]
    fn empty_module_builds() {
        let m = define_module("empty").build();
        assert_eq!(m.name, "empty");
        assert_eq!(m.provider_count(), 0);
    }

    #[test]
    fn service_value_registers_into_container() {
        let m = define_module("users")
            .prefix("/users")
            .service_value(Greeter("hi".into()))
            .build();

        assert_eq!(m.prefix, "/users");
        assert_eq!(m.provider_count(), 1);

        let c = m.register_into(Container::builder()).build().unwrap();
        assert_eq!(c.resolve::<Greeter>().0, "hi");
    }

    #[test]
    fn service_factory_registers_lazily() {
        let m = define_module("svc")
            .service_factory::<Greeter, _>(|_| Arc::new(Greeter("lazy".into())))
            .build();

        let c = m.register_into(Container::builder()).build().unwrap();
        assert_eq!(c.resolve::<Greeter>().0, "lazy");
    }

    #[test]
    fn service_factory_can_depend_on_earlier_value() {
        struct Repo {
            who: String,
        }

        let m = define_module("svc")
            .service_value(Greeter("there".into()))
            .service_factory::<Repo, _>(|c| {
                let g = c.resolve::<Greeter>();
                Arc::new(Repo { who: g.0.clone() })
            })
            .build();

        let c = m.register_into(Container::builder()).build().unwrap();
        assert_eq!(c.resolve::<Repo>().who, "there");
    }

    #[test]
    fn transient_provider_fires_per_resolve() {
        let m = define_module("svc")
            .transient::<Counter, _>(|_| Counter(std::sync::atomic::AtomicUsize::new(0)))
            .build();
        let c = m.register_into(Container::builder()).build().unwrap();

        let a = c.resolve::<Counter>();
        let b = c.resolve::<Counter>();
        assert!(!Arc::ptr_eq(&a, &b), "transients must not share an Arc");
    }

    #[test]
    fn sub_module_providers_are_folded_in() {
        let inner = define_module("inner")
            .service_value(Greeter("inner".into()))
            .build();
        let outer = define_module("outer").sub_module(inner).build();

        assert_eq!(outer.provider_count(), 1);

        let c = outer.register_into(Container::builder()).build().unwrap();
        assert_eq!(c.resolve::<Greeter>().0, "inner");
    }

    #[test]
    fn two_modules_can_compose_into_one_container() {
        struct A(u32);
        struct B(u32);

        let mod_a = define_module("a").service_value(A(1)).build();
        let mod_b = define_module("b").service_value(B(2)).build();

        let mut builder = Container::builder();
        builder = mod_a.register_into(builder);
        builder = mod_b.register_into(builder);
        let c = builder.build().unwrap();

        assert_eq!(c.resolve::<A>().0, 1);
        assert_eq!(c.resolve::<B>().0, 2);
    }

    #[test]
    fn conflicting_providers_in_separate_modules_fail_build() {
        let mod_a = define_module("a")
            .service_value(Greeter("a".into()))
            .build();
        let mod_b = define_module("b")
            .service_value(Greeter("b".into()))
            .build();

        let mut builder = Container::builder();
        builder = mod_a.register_into(builder);
        builder = mod_b.register_into(builder);

        let err = builder.build().unwrap_err();
        assert_eq!(err.code, "RK_E_AMBIGUOUS_BIND");
    }

    #[test]
    fn provider_type_names_includes_sub_modules() {
        struct A;
        struct B;
        let inner = define_module("inner").service_value(B).build();
        let outer = define_module("outer")
            .service_value(A)
            .sub_module(inner)
            .build();

        let names = outer.provider_type_names();
        assert_eq!(names.len(), 2);
        assert!(names.iter().any(|n| n.ends_with("::A")));
        assert!(names.iter().any(|n| n.ends_with("::B")));
    }
}
