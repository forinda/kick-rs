//! Axum handlers — the boundary between HTTP and the service layer.

use super::model::{CreateUser, UpdateUser, User};
use super::service::{UserError, UserService};
use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use kick_rs::{HttpError, HttpResult, Inject, KickError};
use uuid::Uuid;

/// Map a service-layer `UserError` into the framework's structured
/// error type. The HTTP layer then renders it as RFC 7807 problem JSON.
fn to_http(e: UserError) -> HttpError {
    let ke = match &e {
        UserError::NotFound => KickError::new("RK_A_USER_NOT_FOUND", e.to_string()),
        UserError::DuplicateEmail => KickError::new("RK_A_DUPLICATE_EMAIL", e.to_string()),
        UserError::Db(db) => {
            KickError::new("RK_A_DB", "database error").with_context("sqlx", db.to_string())
        }
    };
    HttpError::from(ke)
}

#[utoipa::path(
    get,
    path = "/users",
    responses(
        (status = 200, description = "List users", body = [User]),
    ),
    tag = "users",
)]
pub async fn list(svc: Inject<UserService>) -> HttpResult<Json<Vec<User>>> {
    Ok(Json(svc.list().await.map_err(to_http)?))
}

#[utoipa::path(
    get,
    path = "/users/{id}",
    params(("id" = Uuid, Path, description = "user id")),
    responses(
        (status = 200, description = "User",         body = User),
        (status = 404, description = "Not found"),
    ),
    tag = "users",
)]
pub async fn show(Path(id): Path<Uuid>, svc: Inject<UserService>) -> HttpResult<Json<User>> {
    Ok(Json(svc.show(id).await.map_err(to_http)?))
}

#[utoipa::path(
    post,
    path = "/users",
    request_body = CreateUser,
    responses(
        (status = 201, description = "Created",          body = User),
        (status = 409, description = "Duplicate email"),
    ),
    tag = "users",
)]
pub async fn create(
    svc: Inject<UserService>,
    Json(body): Json<CreateUser>,
) -> HttpResult<(StatusCode, Json<User>)> {
    let user = svc.create(body).await.map_err(to_http)?;
    Ok((StatusCode::CREATED, Json(user)))
}

#[utoipa::path(
    patch,
    path = "/users/{id}",
    params(("id" = Uuid, Path, description = "user id")),
    request_body = UpdateUser,
    responses(
        (status = 200, description = "Updated",   body = User),
        (status = 404, description = "Not found"),
    ),
    tag = "users",
)]
pub async fn update(
    Path(id): Path<Uuid>,
    svc: Inject<UserService>,
    Json(body): Json<UpdateUser>,
) -> HttpResult<Json<User>> {
    Ok(Json(svc.update(id, body).await.map_err(to_http)?))
}

#[utoipa::path(
    delete,
    path = "/users/{id}",
    params(("id" = Uuid, Path, description = "user id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found"),
    ),
    tag = "users",
)]
pub async fn delete(Path(id): Path<Uuid>, svc: Inject<UserService>) -> HttpResult<StatusCode> {
    svc.delete(id).await.map_err(to_http)?;
    Ok(StatusCode::NO_CONTENT)
}
