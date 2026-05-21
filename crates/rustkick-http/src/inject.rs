//! `Inject<T>` — axum extractor backed by [`rustkick_core::Container`].
//!
//! See [`ARCHITECTURE.md` §2.1](../ARCHITECTURE.md#21-inject-extractor) for
//! the full FromRequestParts implementation plan.

use std::ops::Deref;
use std::sync::Arc;

/// Inject a singleton/transient/request-scoped DI value into a handler.
///
/// ```ignore
/// async fn list(svc: Inject<UserService>) -> Json<Vec<User>> { /* … */ }
/// ```
pub struct Inject<T: ?Sized + 'static>(pub Arc<T>);

impl<T: ?Sized + 'static> Deref for Inject<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized + 'static> Clone for Inject<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

// The `FromRequestParts` impl lands in Phase 1, once `Container` exposes
// concrete `resolve()` semantics. Reserved here so the public type is
// stable from day one.
