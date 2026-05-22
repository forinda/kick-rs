//! Wire types for the `users` resource.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Stored representation, also the JSON shape returned by the API.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Body for `POST /users`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateUser {
    pub email: String,
    pub name: String,
}

/// Body for `PATCH /users/:id`. Both fields optional — only the
/// supplied ones are updated.
#[derive(Debug, Deserialize, Default, ToSchema)]
pub struct UpdateUser {
    pub email: Option<String>,
    pub name: Option<String>,
}
