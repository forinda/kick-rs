//! `users` resource — routes, services, model.
//!
//! See [`define`] for the wired-up [`kick_rs::Module`] this file exports.

pub mod handlers;
pub mod model;
pub mod repository;
pub mod service;

use kick_rs::{define_module, paths, Module};
use repository::UserRepository;
use service::UserService;

/// Build the users module — registers the repository + service in DI
/// and binds the five CRUD routes under `/users`.
///
/// `#[service]` on `UserRepository` and `UserService` generates the
/// container-construction logic, so registration is a single
/// `.service::<T>()` call per type. `paths!(...)` registers each
/// handler's `#[utoipa::path]` metadata so the spec served at
/// `/openapi.json` is built from the same list of handler names.
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
        .openapi_paths(paths!(
            handlers::list,
            handlers::show,
            handlers::create,
            handlers::update,
            handlers::delete
        ))
        .build()
}
