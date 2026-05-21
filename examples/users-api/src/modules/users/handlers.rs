//! Axum handlers — the boundary between HTTP and the service layer.

use super::model::{CreateUser, UpdateUser, User};
use super::service::{UserError, UserService};
use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use rustkick::{HttpError, HttpResult, Inject, KickError};
use uuid::Uuid;

/// Map a service-layer `UserError` into the framework's structured
/// error type. The HTTP layer then renders it as RFC 7807 problem JSON.
fn to_http(e: UserError) -> HttpError {
    let ke = match &e {
        UserError::NotFound => KickError::new("RK_A_USER_NOT_FOUND", e.to_string()),
        UserError::DuplicateEmail => KickError::new("RK_A_DUPLICATE_EMAIL", e.to_string()),
        UserError::Db(db) => KickError::new("RK_A_DB", "database error")
            .with_context("sqlx", db.to_string()),
    };
    HttpError::from(ke)
}

pub async fn list(svc: Inject<UserService>) -> HttpResult<Json<Vec<User>>> {
    Ok(Json(svc.list().await.map_err(to_http)?))
}

pub async fn show(
    Path(id): Path<Uuid>,
    svc: Inject<UserService>,
) -> HttpResult<Json<User>> {
    Ok(Json(svc.show(id).await.map_err(to_http)?))
}

pub async fn create(
    svc: Inject<UserService>,
    Json(body): Json<CreateUser>,
) -> HttpResult<(StatusCode, Json<User>)> {
    let user = svc.create(body).await.map_err(to_http)?;
    Ok((StatusCode::CREATED, Json(user)))
}

pub async fn update(
    Path(id): Path<Uuid>,
    svc: Inject<UserService>,
    Json(body): Json<UpdateUser>,
) -> HttpResult<Json<User>> {
    Ok(Json(svc.update(id, body).await.map_err(to_http)?))
}

pub async fn delete(
    Path(id): Path<Uuid>,
    svc: Inject<UserService>,
) -> HttpResult<StatusCode> {
    svc.delete(id).await.map_err(to_http)?;
    Ok(StatusCode::NO_CONTENT)
}
