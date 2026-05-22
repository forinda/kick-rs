//! `users` resource — routes, services, model.
//!
//! See [`define`] for the wired-up [`kick_rs::Module`] this file exports.

pub mod handlers;
pub mod model;
pub mod repository;
pub mod service;

use kick_rs::{define_module, Module};
use repository::UserRepository;
use service::UserService;

/// Build the users module — registers the repository + service in DI
/// and binds the five CRUD routes under `/users`.
///
/// `#[service]` on `UserRepository` and `UserService` generates the
/// container-construction logic, so registration is a single
/// `.service::<T>()` call per type.
pub fn define() -> Module {
    define_module("users")
        .prefix("/users")
        .service::<UserRepository>()
        .service::<UserService>()
        .get("/", handlers::list)
        .get("/:id", handlers::show)
        .post("/", handlers::create)
        .patch("/:id", handlers::update)
        .delete("/:id", handlers::delete)
        .build()
}
