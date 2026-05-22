//! Handlers — note the *complete absence* of tenant-routing logic.
//! Each handler takes `Ctx<TenantDb>` and runs unqualified SQL like
//! `SELECT * FROM posts`. The right schema is selected by the
//! per-tenant pool's `search_path`, set up once in
//! `tenancy::registry::build_tenant_pool`.

use crate::modules::posts::model::{CreatePost, Post};
use crate::tenancy::TenantDb;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use kick_rs::{Ctx, HttpError, HttpResult, KickError};
use uuid::Uuid;

fn db_err(e: sqlx::Error) -> HttpError {
    HttpError::from(
        KickError::new("RK_A_DB", format!("postgres error: {e}"))
            .with_source(e),
    )
}

fn not_found() -> HttpError {
    HttpError::from(
        KickError::new("RK_A_POST_NOT_FOUND", "post not found"),
    )
}

pub async fn list(db: Ctx<TenantDb>) -> HttpResult<Json<Vec<Post>>> {
    let rows = sqlx::query_as::<_, Post>(
        "SELECT id, title, body, created_at FROM posts ORDER BY created_at DESC",
    )
    .fetch_all(db.pool())
    .await
    .map_err(db_err)?;
    Ok(Json(rows))
}

pub async fn show(
    Path(id): Path<Uuid>,
    db: Ctx<TenantDb>,
) -> HttpResult<Json<Post>> {
    let row = sqlx::query_as::<_, Post>(
        "SELECT id, title, body, created_at FROM posts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await
    .map_err(db_err)?;
    row.map(Json).ok_or_else(not_found)
}

pub async fn create(
    db: Ctx<TenantDb>,
    Json(body): Json<CreatePost>,
) -> HttpResult<(StatusCode, Json<Post>)> {
    let post = Post {
        id: Uuid::now_v7(),
        title: body.title,
        body: body.body,
        created_at: Utc::now(),
    };
    sqlx::query(
        "INSERT INTO posts (id, title, body, created_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(post.id)
    .bind(&post.title)
    .bind(&post.body)
    .bind(post.created_at)
    .execute(db.pool())
    .await
    .map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(post)))
}

pub async fn delete(
    Path(id): Path<Uuid>,
    db: Ctx<TenantDb>,
) -> HttpResult<StatusCode> {
    let res = sqlx::query("DELETE FROM posts WHERE id = $1")
        .bind(id)
        .execute(db.pool())
        .await
        .map_err(db_err)?;
    if res.rows_affected() == 0 {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}
