//! sqlx-backed repository for the `users` table.

use super::model::User;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// All DB access for users flows through this struct. Held in DI as a
/// singleton; cheap to clone (an Arc<PgPool> internally).
pub struct UserRepository {
    pool: Arc<PgPool>,
}

impl UserRepository {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    pub async fn find_all(&self) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, email, name, created_at, updated_at \
             FROM users \
             ORDER BY created_at DESC",
        )
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            "SELECT id, email, name, created_at, updated_at \
             FROM users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn insert(&self, user: &User) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO users (id, email, name, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.name)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn update(
        &self,
        id: Uuid,
        email: Option<&str>,
        name: Option<&str>,
    ) -> Result<Option<User>, sqlx::Error> {
        // Naive but readable — issue a single UPDATE that COALESCEs the
        // fields, then SELECT to return the latest row. A production
        // repo would batch this; for an example, clarity wins.
        sqlx::query(
            "UPDATE users SET \
               email      = COALESCE($2, email), \
               name       = COALESCE($3, name), \
               updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(id)
        .bind(email)
        .bind(name)
        .execute(&*self.pool)
        .await?;
        self.find_by_id(id).await
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let res = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(&*self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}
