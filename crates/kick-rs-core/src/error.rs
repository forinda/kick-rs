//! Structured framework error type. Mirrors KickJS [`KickError`].
//!
//! All boot-time and runtime framework errors carry a stable `code`
//! (e.g. `RK_E_UNKNOWN_TOKEN`), a human message, and an optional fix hint.
//! See [`ARCHITECTURE.md`](../ARCHITECTURE.md#6-error-model) for the code
//! prefix table and the full boot-time error matrix.

use std::collections::BTreeMap;

/// Convenience alias.
pub type KickResult<T> = Result<T, KickError>;

/// Framework-wide structured error.
#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct KickError {
    /// Stable machine-readable error code (e.g. `RK_E_UNKNOWN_TOKEN`).
    pub code: &'static str,
    /// Human-readable summary.
    pub message: String,
    /// Optional actionable suggestion for the developer.
    pub fix_hint: Option<String>,
    /// Wrapped lower-level error, if any.
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
    /// Free-form extension fields surfaced in problem-details JSON.
    pub context: BTreeMap<String, String>,
}

impl KickError {
    /// Create a new error with just a code + message.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            fix_hint: None,
            source: None,
            context: BTreeMap::new(),
        }
    }

    /// Attach a fix hint.
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.fix_hint = Some(hint.into());
        self
    }

    /// Attach a wrapped source error.
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Attach a context key/value pair.
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
}
