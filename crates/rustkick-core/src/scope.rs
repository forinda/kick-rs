//! Provider lifetime scopes — singleton, transient, request.

/// Lifetime for a DI provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// One instance for the lifetime of the [`Container`](crate::Container).
    Singleton,
    /// A new instance per `resolve()` call.
    Transient,
    /// One instance per request, stored on the request's extensions map.
    Request,
}
