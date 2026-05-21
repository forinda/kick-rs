//! HTTP error wrapper — adapts [`rustkick_core::KickError`] into an
//! axum [`IntoResponse`] producing RFC 7807 Problem Details JSON.
//!
//! The orphan rule blocks impl'ing `IntoResponse` directly on `KickError`
//! (both types live outside this crate), so we wrap.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rustkick_core::KickError;
use serde_json::json;

/// Newtype wrapping [`KickError`] so we can implement [`IntoResponse`].
///
/// `From<KickError>` is provided so handler functions returning
/// `HttpResult<T>` can use the `?` operator with `KickError` values
/// produced by extractors or the container.
#[derive(Debug)]
pub struct HttpError(pub KickError);

impl From<KickError> for HttpError {
    fn from(e: KickError) -> Self {
        HttpError(e)
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for HttpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source.as_ref().map(|s| s.as_ref() as _)
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        // Phase 1.4 ships a single mapping. Per-code status mapping
        // (e.g., RK_H_BAD_BODY -> 400) lands when validation middleware
        // does in Phase 5.
        let status = StatusCode::INTERNAL_SERVER_ERROR;
        let body = json!({
            "type":   format!("https://errors.rustkick.dev/{}", self.0.code),
            "title":  self.0.message,
            "status": status.as_u16(),
            "detail": self.0.fix_hint,
            "code":   self.0.code,
        });
        (status, Json(body)).into_response()
    }
}

/// Convenience alias for handlers — `Ok(Json(...))` / `Err(KickError)?`.
pub type HttpResult<T> = std::result::Result<T, HttpError>;
