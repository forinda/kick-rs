//! Per-request context. See [`SPEC.md` §4.7](../SPEC.md#47-requestcontext-ctx).

use std::marker::PhantomData;

/// Per-request context — typed path params, headers, and a typed
/// extension map populated by context contributors.
pub struct RequestContext {
    // Real shape lands in Phase 1.
}

impl RequestContext {
    /// Lookup a value populated by an upstream contributor.
    pub fn get<T: 'static>(&self) -> &T {
        todo!("RequestContext::get — implementation pending Phase 1")
    }

    /// Lookup, returning `None` if no contributor produced `T`.
    pub fn try_get<T: 'static>(&self) -> Option<&T> {
        todo!("RequestContext::try_get — implementation pending Phase 1")
    }
}

/// Typed-param wrapper around [`RequestContext`].
///
/// Used as a handler argument when the route has typed path parameters:
///
/// ```ignore
/// async fn show(ctx: Ctx<ShowParams>) -> Json<User> { /* … */ }
/// ```
pub struct Ctx<P = ()> {
    /// Typed path parameters parsed from the route.
    pub params: P,
    _marker: PhantomData<()>,
}
