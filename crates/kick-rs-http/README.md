# kick-rs-http

> Axum integration for the [kick-rs](https://github.com/forinda/kick-rs)
> framework — `bootstrap()` runner, `Inject<T>` extractor, `HttpModule`
> with `.get/.post/.put/.patch/.delete`.

[![crates.io](https://img.shields.io/crates/v/kick-rs-http.svg)](https://crates.io/crates/kick-rs-http)
[![docs.rs](https://docs.rs/kick-rs-http/badge.svg)](https://docs.rs/kick-rs-http)
[![license](https://img.shields.io/crates/l/kick-rs-http.svg)](https://github.com/forinda/kick-rs/blob/main/LICENSE)

Builds on [`kick-rs-core`](https://crates.io/crates/kick-rs-core) to
wire DI + modules + adapter lifecycle to an `axum::Router`, plus a
cooperative-shutdown serve loop with per-adapter timeout budgets.

## Most users want [`kick-rs`](https://crates.io/crates/kick-rs)

The umbrella `kick-rs` crate re-exports everything in this crate plus
the core primitives, so app code can write a single `use kick-rs::*;`
import. Depending on `kick-rs-http` directly only makes sense if
you're building a library that needs HTTP integration but wants to
avoid pulling in the umbrella indirection. If you're writing an app,
use the umbrella.

## What's in this crate

| Module       | What it provides                                                          |
|--------------|---------------------------------------------------------------------------|
| `bootstrap`  | `bootstrap().listen()` — full lifecycle: container build → adapter topo-sort → before_mount → before_start → serve with Ctrl-C → after_start → cooperative shutdown |
| `module`     | `define_module()` HTTP builder with `.prefix/.get/.post/.put/.patch/.delete/.sub_module` |
| `inject`     | `Inject<T>` axum `FromRequestParts` extractor backed by the container     |
| `error`      | `HttpError` newtype around `KickError` with RFC 7807 problem-details `IntoResponse` |
| `context`    | `RequestContext` / `Ctx<P>` — per-request typed context (extended in Phase 4 with contributor outputs) |
| `plugins`    | Built-in `HttpPlugin`s: `RequestIdPlugin`, `RequestLoggerPlugin`, `CorsPlugin`, `CompressionPlugin`, `HelmetPlugin`, `TraceContextPlugin` — all feature-gated |
| `openapi`    | (Feature `openapi`) `OpenApiPlugin` that serves a `utoipa::openapi::OpenApi` spec at a configurable path |

## Built-in plugins

All four are enabled by default. Disable any subset with
`default-features = false` and pick à-la-carte:

```toml
kick-rs-http = { version = "0.1.0-alpha.1", default-features = false, features = ["plugin-request-id"] }
```

| Plugin                | Feature gate            | Phase           | What it does                                                                          |
|-----------------------|-------------------------|-----------------|---------------------------------------------------------------------------------------|
| `RequestIdPlugin`     | `plugin-request-id`     | `BeforeGlobal`  | Reads inbound `X-Request-Id` or generates UUIDv7; mirrors to response header          |
| `RequestLoggerPlugin` | `plugin-request-logger` | `AfterGlobal`   | Emits one `tracing::info!` per request (method/path/status/elapsed_us)                |
| `CorsPlugin`          | `plugin-cors`           | `BeforeGlobal`  | Wraps `tower_http::cors::CorsLayer` (`::permissive()` / `::with_layer(...)`)          |
| `CompressionPlugin`   | `plugin-compression`    | `AfterGlobal`   | gzip / br / deflate / zstd response compression via `tower_http`                      |
| `HelmetPlugin`        | `plugin-helmet`         | `BeforeGlobal`  | Baseline security headers (nosniff, frame-deny, HSTS, referrer-policy, COOP/CORP, …)  |
| `TraceContextPlugin`  | `plugin-trace-context`  | `BeforeGlobal`  | W3C `traceparent` parsing + propagation; exposes `TraceContext` extension             |

Mount with `bootstrap().http_plugin(RequestIdPlugin::default())`.

## OpenAPI (opt-in)

Off by default — enable with the `openapi` cargo feature, which pulls
in [`utoipa`]. Adopters assemble their own `OpenApi` value via
`#[derive(utoipa::OpenApi)]` and hand it to `OpenApiPlugin::new`:

```rust,ignore
use kick_rs_http::openapi::OpenApiPlugin;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(paths(crate::users::list, crate::users::get))]
struct ApiDoc;

bootstrap()
    .http_plugin(OpenApiPlugin::new(ApiDoc::openapi()))
    .module(users_module())
    .listen("0.0.0.0:3000").await
```

The spec is serialized to JSON once at construction and served as
`application/json` at `/openapi.json` (or pass `.with_path("/api/spec")`).

### Auto-collecting from modules

Skip the parallel `#[derive(OpenApi)]` block by registering each
handler's `__path_<name>` type directly on its module:

```rust,ignore
use kick_rs_http::openapi::OpenApiPlugin;
use utoipa::openapi::InfoBuilder;

#[utoipa::path(get, path = "/users/{id}", responses(...))]
async fn get_user(/* ... */) { /* ... */ }

let users = define_module("users")
    .get("/users/:id", get_user)
    .openapi_path::<__path_get_user>()
    .build();

let plugin = OpenApiPlugin::from_modules(
    InfoBuilder::new().title("My API").version("1.0").build(),
    [&users],
);

bootstrap().http_plugin(plugin).module(users).listen(addr).await
```

Sub-modules are walked recursively, so a top-level module nesting
half the app is enough.

### Bulk registration via `paths!`

With the `macros` feature on, drop the per-handler turbofish by
listing handlers up front:

```rust,ignore
use kick_rs::paths;

let users = define_module("users")
    .get("/users/:id", get_user)
    .post("/users",    create_user)
    .openapi_paths(paths!(get_user, create_user))
    .build();
```

Qualified paths work too — `paths!(api::v1::list, get_one)` resolves
to `api::v1::__path_list` and `__path_get_one` respectively.

[`utoipa`]: https://docs.rs/utoipa

## DevTools `/__debug` endpoint (opt-in)

Off by default — turn it on with the `devtools` cargo feature **and**
a `.with_devtools()` call on the bootstrap (two opt-ins on purpose,
to make it hard to accidentally ship in production):

```toml
kick-rs-http = { version = "0.1.0-alpha.1", features = ["devtools"] }
```

```rust,ignore
bootstrap()
    .module(users::define())
    .with_devtools()             // or .with_devtools_at("/internal/state")
    .listen(addr).await
```

`GET /__debug` then returns a JSON snapshot of the assembled app:

```json
{
  "framework": "kick-rs",
  "version":   "0.1.0-alpha.1",
  "modules":   [{ "name": "users", "prefix": "/users", "routes": 5, "sub_modules": [] }],
  "plugins":   [{ "name": "request-id" }, { "name": "openapi" }],
  "adapters":  [],
  "contributors": { "count": 2 }
}
```

The snapshot is serialized once at boot, so per-request cost is just
a refcount bump and a string clone.

## Quick example

```rust
use kick_rs_http::{bootstrap, define_module, Inject, KickResult};
use axum::Json;

struct HelloService;
impl HelloService {
    fn greet(&self, name: &str) -> serde_json::Value {
        serde_json::json!({ "message": format!("Hello {name}!") })
    }
}

async fn index(svc: Inject<HelloService>) -> Json<serde_json::Value> {
    Json(svc.greet("World"))
}

#[tokio::main]
async fn main() -> KickResult<()> {
    bootstrap()
        .module(
            define_module("hello")
                .prefix("/hello")
                .service_value(HelloService)
                .get("/", index)
                .build()
        )
        .listen("0.0.0.0:3000")
        .await
}
```

A full CRUD example over Postgres lives in
[`examples/users-api`](https://github.com/forinda/kick-rs/tree/main/examples/users-api).

## Cooperative shutdown

`bootstrap().listen()` installs a Ctrl-C handler that triggers graceful
shutdown. Each registered adapter's `shutdown()` runs concurrently
under `futures::join_all` with a per-adapter timeout
(`shutdown_timeout()` on the builder, default 10s) so one slow flush
can't block siblings. Modeled after KickJS's `Promise.allSettled`-based
shutdown.

## Install

```toml
[dependencies]
kick-rs-http = "0.1.0-alpha.1"
```

## Status

Early — at `0.0.x`. API surface is reserved but may shift before
`v0.1.0`. See
[`RELEASE.md`](https://github.com/forinda/kick-rs/blob/main/RELEASE.md)
for the versioning model.

## License

MIT — see the workspace root.
