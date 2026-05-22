//! `users` resource — routes, services, model.
//!
//! See [`define`] for the wired-up [`kick_rs::Module`] this file exports.

pub mod handlers;
pub mod model;
pub mod repository;
pub mod service;

use repository::UserRepository;
use kick_rs::{define_module, Module};
use service::UserService;
use std::sync::Arc;

/// Build the users module — registers the repository + service in DI
/// and binds the five CRUD routes under `/users`.
pub fn define() -> Module {
    define_module("users")
        .prefix("/users")
        .service_factory::<UserRepository, _>(|c| {
            let pool = c.resolve::<sqlx::PgPool>();
            Arc::new(UserRepository::new(pool))
        })
        .service_factory::<UserService, _>(|c| {
            let repo = c.resolve::<UserRepository>();
            Arc::new(UserService::new(repo))
        })
        .get("/", handlers::list)
        .get("/:id", handlers::show)
        .post("/", handlers::create)
        .patch("/:id", handlers::update)
        .delete("/:id", handlers::delete)
        .build()
}
