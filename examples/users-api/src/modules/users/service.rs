//! Application-layer service. Wraps the repository, applies any
//! domain rules (none yet beyond uuid generation), and returns
//! typed errors callers can translate to HTTP responses.

use super::model::{CreateUser, UpdateUser, User};
use super::repository::UserRepository;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

/// Service-layer errors. Kept simple — handler layer maps to HTTP.
#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("user not found")]
    NotFound,
    #[error("user with email already exists")]
    DuplicateEmail,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub struct UserService {
    repo: Arc<UserRepository>,
}

impl UserService {
    pub fn new(repo: Arc<UserRepository>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<User>, UserError> {
        Ok(self.repo.find_all().await?)
    }

    pub async fn show(&self, id: Uuid) -> Result<User, UserError> {
        self.repo.find_by_id(id).await?.ok_or(UserError::NotFound)
    }

    pub async fn create(&self, input: CreateUser) -> Result<User, UserError> {
        let now = Utc::now();
        let user = User {
            id: Uuid::now_v7(),
            email: input.email,
            name: input.name,
            created_at: now,
            updated_at: now,
        };
        match self.repo.insert(&user).await {
            Ok(()) => Ok(user),
            Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
                Err(UserError::DuplicateEmail)
            }
            Err(e) => Err(UserError::Db(e)),
        }
    }

    pub async fn update(&self, id: Uuid, patch: UpdateUser) -> Result<User, UserError> {
        let updated = self
            .repo
            .update(id, patch.email.as_deref(), patch.name.as_deref())
            .await?;
        updated.ok_or(UserError::NotFound)
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), UserError> {
        if self.repo.delete(id).await? {
            Ok(())
        } else {
            Err(UserError::NotFound)
        }
    }
}
