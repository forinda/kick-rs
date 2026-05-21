//! Introspection contract — feeds the (future) `/__debug` DevTools endpoint.
//!
//! See [`SPEC.md` §4.11](../SPEC.md#411-introspection-devtools-contract).

use serde::{Deserialize, Serialize};

/// What kind of component is being introspected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntrospectionKind {
    /// A [`crate::Module`].
    Module,
    /// An [`crate::Adapter`].
    Adapter,
    /// A [`crate::Plugin`].
    Plugin,
}

/// One snapshot per introspected component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntrospectionSnapshot {
    /// Wire-format version. Bump on breaking shape changes.
    pub protocol_version: u32,
    /// What kind of component this snapshot describes.
    pub kind: IntrospectionKind,
    /// Stable name of the component.
    pub name: String,
    /// Free-form component-specific state — DevTools renders as JSON.
    pub state: serde_json::Value,
    /// DI tokens this component owns or relies on.
    pub tokens: Vec<String>,
    /// Best-effort memory footprint, if measurable.
    pub memory_bytes: Option<usize>,
}

/// Opt-in hook: adapters/plugins implementing this surface state to
/// DevTools and `cargo rustkick info`.
pub trait Introspect {
    /// Produce a snapshot of the current state.
    fn introspect(&self) -> IntrospectionSnapshot;
}
