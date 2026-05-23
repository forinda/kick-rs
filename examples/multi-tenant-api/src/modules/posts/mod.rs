//! `posts` resource — same shape as kick-rs's `users-api` example but
//! the repository pulls a per-tenant pool from `Ctx<TenantDb>` instead
//! of a global one. The CRUD logic is identical; only the data source
//! differs per request.

pub mod handlers;
pub mod model;

use kick_rs::{define_module, paths, Module};

pub fn define() -> Module {
    define_module("posts")
        .prefix("/posts")
        .get("/", handlers::list)
        .get("/:id", handlers::show)
        .post("/", handlers::create)
        .delete("/:id", handlers::delete)
        .openapi_paths(paths!(
            handlers::list,
            handlers::show,
            handlers::create,
            handlers::delete
        ))
        .build()
}
