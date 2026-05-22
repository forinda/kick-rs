//! Integration tests for `#[get]` / `#[post]` / etc.
//!
//! These tests live in `tests/` so we can use both the macro crate AND
//! `kick-rs-http` together — a proc-macro crate's unit tests can't
//! consume its own macros.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Json;
use kick_rs_http::{bootstrap, define_module};
use kick_rs_macros::{delete, get, patch, post, put};
use tower::ServiceExt;

// ── Plain GET ──────────────────────────────────────────────────────────────

#[get("/ping")]
async fn ping() -> &'static str {
    "pong"
}

#[tokio::test]
async fn get_macro_mounts_via_handler() {
    let m = define_module("t").handler(ping_route).build();
    let (router, _) = bootstrap().module(m).into_router().unwrap();

    let res = router
        .oneshot(Request::get("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 16).await.unwrap();
    assert_eq!(&body[..], b"pong");
}

// ── Each method ────────────────────────────────────────────────────────────

#[get("/x")]
async fn x_get() -> &'static str {
    "G"
}
#[post("/x")]
async fn x_post() -> &'static str {
    "P"
}
#[put("/x")]
async fn x_put() -> &'static str {
    "PU"
}
#[patch("/x")]
async fn x_patch() -> &'static str {
    "PA"
}
#[delete("/x")]
async fn x_delete() -> &'static str {
    "D"
}

#[tokio::test]
async fn each_method_macro_dispatches_correctly() {
    let m = define_module("t")
        .handlers([
            x_get_route,
            x_post_route,
            x_put_route,
            x_patch_route,
            x_delete_route,
        ])
        .build();
    let (router, _) = bootstrap().module(m).into_router().unwrap();

    for (method, expected) in [
        (Method::GET, "G"),
        (Method::POST, "P"),
        (Method::PUT, "PU"),
        (Method::PATCH, "PA"),
        (Method::DELETE, "D"),
    ] {
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method.clone())
                    .uri("/x")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "method {method}");
        let body = axum::body::to_bytes(res.into_body(), 16).await.unwrap();
        assert_eq!(&body[..], expected.as_bytes(), "method {method}");
    }
}

// ── Path params still flow through ─────────────────────────────────────────

#[get("/echo/:id")]
async fn echo_id(axum::extract::Path(id): axum::extract::Path<String>) -> Json<String> {
    Json(id)
}

#[tokio::test]
async fn path_params_reach_the_handler() {
    let m = define_module("t").handler(echo_id_route).build();
    let (router, _) = bootstrap().module(m).into_router().unwrap();

    let res = router
        .oneshot(Request::get("/echo/abc").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 64).await.unwrap();
    assert_eq!(&body[..], br#""abc""#);
}

// ── Prefix interaction ─────────────────────────────────────────────────────
//
// `#[get(...)]` mounts at the literal path it's given. The module's
// `.prefix("/api")` only affects the existing `.get(path, fn)` style
// (which prepends the prefix internally). With macro-driven `.handler`,
// the user includes the full path in the attribute. Documented behavior.

#[get("/api/v1/health")]
async fn health() -> &'static str {
    "ok"
}

#[tokio::test]
async fn macro_route_uses_literal_path_not_module_prefix() {
    let m = define_module("t")
        .prefix("/should-be-ignored")
        .handler(health_route)
        .build();
    let (router, _) = bootstrap().module(m).into_router().unwrap();

    // The macro mounts at its literal path; the module prefix is NOT
    // applied (this is by design — handler() is for macro-driven
    // routes; .get(path, fn) is for prefix-respecting routes).
    let res = router
        .oneshot(Request::get("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
