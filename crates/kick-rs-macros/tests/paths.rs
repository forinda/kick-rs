//! End-to-end test for the `paths!` proc-macro. The macro is only
//! useful in combination with `kick-rs-http`'s `openapi` feature and
//! a `#[utoipa::path(...)]`-annotated handler, so the whole flow is
//! exercised here.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use kick_rs_http::openapi::OpenApiPlugin;
use kick_rs_http::{bootstrap, define_module};
use kick_rs_macros::paths;
use tower::ServiceExt;
use utoipa::openapi::InfoBuilder;

#[utoipa::path(
    get,
    path = "/items",
    responses((status = 200, description = "list items"))
)]
async fn list_items() -> &'static str {
    "[]"
}

#[utoipa::path(
    post,
    path = "/items",
    responses((status = 201, description = "create item"))
)]
async fn create_item() -> &'static str {
    "ok"
}

#[tokio::test]
async fn paths_macro_registers_every_handler() {
    let items = define_module("items")
        .get("/items", list_items)
        .post("/items", create_item)
        .openapi_paths(paths!(list_items, create_item))
        .build();

    let plugin = OpenApiPlugin::from_modules(
        InfoBuilder::new()
            .title("paths!-test")
            .version("0.0.1")
            .build(),
        [&items],
    );

    let (router, _) = bootstrap()
        .http_plugin(plugin)
        .module(items)
        .into_router()
        .unwrap();

    let res = router
        .oneshot(Request::get("/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = axum::body::to_bytes(res.into_body(), 65536).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        parsed["paths"]["/items"]["get"].is_object(),
        "expected GET /items: {parsed:#}"
    );
    assert!(
        parsed["paths"]["/items"]["post"].is_object(),
        "expected POST /items: {parsed:#}"
    );
}
