//! Fluent module registry — `define_modules()` builder + `setup(|reg|)`
//! callback for conditional / dynamic mounting.
//!
//! See [`SPEC.md` §4.10](../SPEC.md#410-module-registry-dynamic-mount).
//! KickJS adopters: this is the Rust analog of
//! `defineModules().mount(...).setup(...)`.
//!
//! ```ignore
//! let modules = define_modules()
//!     .mount(users_module())
//!     .mount(health_module())
//!     .setup(|reg| {
//!         if env.feature_billing {
//!             reg.mount(billing_module());
//!         }
//!         reg.mount_if(env.has_admin_portal, admin_module());
//!     });
//!
//! bootstrap().modules(modules).listen(addr).await
//! ```
//!
//! `Bootstrap` exposes the same closure-style hook directly via
//! [`Bootstrap::setup`](crate::Bootstrap::setup) so adopters can splice
//! conditional mounts mid-chain without constructing a `ModuleList`
//! explicitly.

use crate::module::HttpModule;

/// Fluent collection of [`HttpModule`]s.
///
/// Consumed as an iterator by [`Bootstrap::modules`](crate::Bootstrap::modules).
#[derive(Default)]
pub struct ModuleList {
    modules: Vec<HttpModule>,
}

impl ModuleList {
    /// Empty list.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a module. Same shape as [`Bootstrap::module`](crate::Bootstrap::module)
    /// so app code can compose lists separately from the bootstrap call site.
    pub fn mount(mut self, m: HttpModule) -> Self {
        self.modules.push(m);
        self
    }

    /// Conditional append. Convenience over `if cond { list.mount(m) }`.
    pub fn mount_if(mut self, cond: bool, m: HttpModule) -> Self {
        if cond {
            self.modules.push(m);
        }
        self
    }

    /// Run a closure with mutable access to a [`ModuleRegistry`] over
    /// this list — the idiom for conditional / env-driven mounting at
    /// the point of composition.
    pub fn setup<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut ModuleRegistry<'_>),
    {
        let mut reg = ModuleRegistry {
            modules: &mut self.modules,
        };
        f(&mut reg);
        self
    }

    /// Number of modules in the list.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

/// Entry point — start composing a [`ModuleList`].
pub fn define_modules() -> ModuleList {
    ModuleList::new()
}

impl IntoIterator for ModuleList {
    type Item = HttpModule;
    type IntoIter = std::vec::IntoIter<HttpModule>;
    fn into_iter(self) -> Self::IntoIter {
        self.modules.into_iter()
    }
}

impl std::fmt::Debug for ModuleList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleList")
            .field("count", &self.modules.len())
            .finish()
    }
}

/// Borrowed view of a [`ModuleList`] (or `Bootstrap`'s modules) handed
/// to `setup` callbacks. Mutation through this facade keeps the
/// underlying owner's invariants intact.
pub struct ModuleRegistry<'a> {
    pub(crate) modules: &'a mut Vec<HttpModule>,
}

impl<'a> ModuleRegistry<'a> {
    /// Mount a module unconditionally.
    pub fn mount(&mut self, m: HttpModule) {
        self.modules.push(m);
    }

    /// Mount only if `cond` is true. Lets `setup` callbacks read like
    /// the natural condition.
    pub fn mount_if(&mut self, cond: bool, m: HttpModule) {
        if cond {
            self.modules.push(m);
        }
    }

    /// Mount every module from an iterator. Lets a `setup` callback
    /// fold multiple feature flags together cleanly.
    pub fn extend<I: IntoIterator<Item = HttpModule>>(&mut self, ms: I) {
        self.modules.extend(ms);
    }

    /// How many modules are currently registered, including any added
    /// earlier in the current `setup` closure.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether nothing has been mounted yet.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}
