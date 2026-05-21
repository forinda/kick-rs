//! App entry-point builder. See [`SPEC.md` §4.14](../SPEC.md#414-bootstrap).

use rustkick_core::{Adapter, KickResult, Module, Plugin};
use std::sync::Arc;

/// Begin assembling an app.
pub fn bootstrap() -> Bootstrap {
    Bootstrap::default()
}

/// Fluent builder for an app. Terminal method is [`Bootstrap::listen`].
#[derive(Default)]
pub struct Bootstrap {
    modules: Vec<Module>,
    adapters: Vec<Arc<dyn Adapter>>,
    plugins: Vec<Arc<dyn Plugin>>,
}

impl Bootstrap {
    /// Mount a single module.
    pub fn module(mut self, m: Module) -> Self {
        self.modules.push(m);
        self
    }

    /// Mount multiple modules.
    pub fn modules<I: IntoIterator<Item = Module>>(mut self, ms: I) -> Self {
        self.modules.extend(ms);
        self
    }

    /// Register an adapter.
    pub fn adapter<A: Adapter + 'static>(mut self, a: A) -> Self {
        self.adapters.push(Arc::new(a));
        self
    }

    /// Register a plugin.
    pub fn plugin<P: Plugin + 'static>(mut self, p: P) -> Self {
        self.plugins.push(Arc::new(p));
        self
    }

    /// Start the server on `addr`. Real implementation lands in Phase 1.
    pub async fn listen(self, _addr: &str) -> KickResult<()> {
        todo!("Bootstrap::listen — implementation pending Phase 1")
    }
}
