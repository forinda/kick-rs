//! Contributor pipeline integration for axum.
//!
//! At bootstrap time we collect every contributor declared by a module
//! (or sub-module) into a single [`ContributorPipeline`], topo-sort it,
//! and install [`contributors_layer`] on the router. The layer runs the
//! pipeline before every handler and stores the resulting
//! [`ContributorStore`] in the request extensions so handlers can pull
//! values via [`Ctx<T>`] (or any future `RequestContext` accessor).
//!
//! See [`SPEC.md` §4.6](../SPEC.md#46-context-contributor).

use crate::error::HttpError;
use axum::extract::{FromRequestParts, Request};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use kick_rs_core::{ContributorPipeline, ContributorStore, KickError};
use std::ops::Deref;
use std::sync::Arc;

/// Inject a value produced by a [`ContextContributor`](kick_rs_core::ContextContributor)
/// into a handler.
///
/// ```ignore
/// async fn show(tenant: Ctx<Tenant>) -> Json<Tenant> {
///     Json((*tenant).clone())
/// }
/// ```
///
/// Returns an `Arc<T>` clone (cheap reference bump). The contributor
/// that produces `T` must be registered on a module whose contributors
/// are gathered into the bootstrap pipeline — missing producers fail
/// at boot, not at request time.
pub struct Ctx<T: 'static + Send + Sync>(pub Arc<T>);

impl<T: 'static + Send + Sync> Deref for Ctx<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: 'static + Send + Sync> Clone for Ctx<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: 'static + Send + Sync> std::fmt::Debug for Ctx<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx")
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}

#[async_trait::async_trait]
impl<T, S> FromRequestParts<S> for Ctx<T>
where
    T: 'static + Send + Sync,
    S: Send + Sync,
{
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, HttpError> {
        let store = parts.extensions.get::<ContributorStore>().ok_or_else(|| {
            HttpError::from(
                KickError::new(
                    "RK_H_NO_CONTRIBUTOR_STORE",
                    "no ContributorStore in request extensions",
                )
                .with_hint(
                    "Ctx<T> needs the bootstrap()-installed contributors_layer; \
                     attach `contributors_layer(...)` if wiring axum manually.",
                ),
            )
        })?;

        let arc = store.get_arc::<T>().ok_or_else(|| {
            HttpError::from(
                KickError::new(
                    "RK_E_MISSING_CONTRIBUTOR",
                    format!("no contributor produced `{}`", std::any::type_name::<T>()),
                )
                .with_hint(
                    "register a ContextContributor whose `Key` is this type, \
                     either on the module that owns this handler or globally on bootstrap.",
                )
                .with_context("type", std::any::type_name::<T>()),
            )
        })?;

        Ok(Ctx(arc))
    }
}

/// Axum middleware that runs the contributor pipeline against a fresh
/// store and stashes the populated store on the request for downstream
/// handlers and extractors.
///
/// Install via [`Bootstrap`](crate::Bootstrap) — direct use is for
/// adopters wiring axum manually.
pub async fn contributors_middleware(
    pipeline: Arc<ContributorPipeline>,
    mut req: Request,
    next: Next,
) -> Result<Response, HttpError> {
    let mut store = ContributorStore::new();
    pipeline.run(&mut store).await.map_err(HttpError::from)?;
    req.extensions_mut().insert(store);
    Ok(next.run(req).await)
}
