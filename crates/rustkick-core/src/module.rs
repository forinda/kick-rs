//! Module composition — see [`SPEC.md` §4.3](../SPEC.md#43-module).
//!
//! `define_module()` returns a [`ModuleBuilder`] you fluent-chain into a
//! [`Module`]. Routes are stored as transport-agnostic `RouteSpec` values
//! the HTTP crate later wires into an `axum::Router`.

use crate::contributor::AnyContributor;

/// Built module — opaque from the user's POV; HTTP wiring lives in
/// `rustkick-http`.
#[allow(dead_code)] // Fields wired up by HTTP crate + Phase 1 implementation.
pub struct Module {
    /// Stable name (logging, introspection, `depends_on` lookups).
    pub name: String,
    /// Route prefix applied to every route in this module.
    pub prefix: String,
    pub(crate) providers: Vec<ProviderSpec>,
    pub(crate) routes: Vec<RouteSpec>,
    pub(crate) contributors: Vec<Box<dyn AnyContributor>>,
    pub(crate) sub_modules: Vec<Module>,
}

/// Fluent builder for [`Module`].
pub struct ModuleBuilder {
    name: String,
    prefix: String,
    providers: Vec<ProviderSpec>,
    routes: Vec<RouteSpec>,
    contributors: Vec<Box<dyn AnyContributor>>,
    sub_modules: Vec<Module>,
}

/// Entry point — start composing a module.
pub fn define_module(name: impl Into<String>) -> ModuleBuilder {
    ModuleBuilder {
        name: name.into(),
        prefix: String::new(),
        providers: Vec::new(),
        routes: Vec::new(),
        contributors: Vec::new(),
        sub_modules: Vec::new(),
    }
}

impl ModuleBuilder {
    /// URL prefix for every route in this module.
    pub fn prefix(mut self, p: impl Into<String>) -> Self {
        self.prefix = p.into();
        self
    }

    /// Register a service for DI. Concrete wiring lands in Phase 1.
    pub fn service<T: 'static + Send + Sync>(self) -> Self {
        // TODO: append a ProviderSpec referencing T
        self
    }

    /// Mount a sub-module under this module's prefix.
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
            routes: self.routes,
            contributors: self.contributors,
            sub_modules: self.sub_modules,
        }
    }
}

/// Internal — DI provider description carried through the module.
pub(crate) struct ProviderSpec {
    // Real shape lands in Phase 1.
}

/// Internal — transport-agnostic route description. The HTTP crate
/// turns these into `axum::Router` entries.
pub(crate) struct RouteSpec {
    // Real shape lands in Phase 1.
}
