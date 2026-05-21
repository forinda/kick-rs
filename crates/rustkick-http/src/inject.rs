//! `Inject<T>` — axum extractor backed by [`rustkick_core::Container`].
//!
//! At bootstrap time the container is attached to the axum `Router` via an
//! `Extension<Container>` layer. This extractor pulls it out of the request
//! and resolves `T`.
//!
//! See [`ARCHITECTURE.md` §2.1](../ARCHITECTURE.md#21-inject-extractor).

use crate::error::HttpError;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use rustkick_core::{Container, KickError};
use std::ops::Deref;
use std::sync::Arc;

/// Inject a DI value into a handler.
///
/// ```ignore
/// async fn list(svc: Inject<UserService>) -> Json<Vec<User>> { /* … */ }
/// ```
pub struct Inject<T: 'static + Send + Sync>(pub Arc<T>);

impl<T: 'static + Send + Sync> Deref for Inject<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: 'static + Send + Sync> Clone for Inject<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: 'static + Send + Sync> std::fmt::Debug for Inject<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inject").field("type", &std::any::type_name::<T>()).finish()
    }
}

#[async_trait]
impl<T, S> FromRequestParts<S> for Inject<T>
where
    T: 'static + Send + Sync,
    S: Send + Sync,
{
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, HttpError> {
        let container = parts
            .extensions
            .get::<Container>()
            .ok_or_else(|| {
                HttpError::from(
                    KickError::new("RK_H_NO_CONTAINER", "no Container in request extensions")
                        .with_hint(
                            "Inject<T> requires `bootstrap()`-built apps; \
                             attach `Extension(container)` if wiring axum manually.",
                        ),
                )
            })?
            .clone();

        let arc = container.try_resolve::<T>().ok_or_else(|| {
            HttpError::from(
                KickError::new(
                    "RK_E_UNKNOWN_TOKEN",
                    format!("no provider for `{}`", std::any::type_name::<T>()),
                )
                .with_hint(
                    "register the type via `.service_value()`, `.service_factory()`, \
                     or `.transient()` on the module that owns this handler.",
                )
                .with_context("type", std::any::type_name::<T>()),
            )
        })?;

        Ok(Inject(arc))
    }
}
