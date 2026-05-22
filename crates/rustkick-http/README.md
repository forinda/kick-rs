# rustkick-http

> Axum integration for the [rustkick](https://github.com/forinda/rustkick)
> framework — `bootstrap()` runner, `Inject<T>` extractor, `HttpModule`
> with `.get/.post/.put/.patch/.delete`.

[![crates.io](https://img.shields.io/crates/v/rustkick-http.svg)](https://crates.io/crates/rustkick-http)
[![docs.rs](https://docs.rs/rustkick-http/badge.svg)](https://docs.rs/rustkick-http)
[![license](https://img.shields.io/crates/l/rustkick-http.svg)](https://github.com/forinda/rustkick/blob/main/LICENSE)

Builds on [`rustkick-core`](https://crates.io/crates/rustkick-core) to
wire DI + modules + adapter lifecycle to an `axum::Router`, plus a
cooperative-shutdown serve loop with per-adapter timeout budgets.

## Most users want [`rustkick`](https://crates.io/crates/rustkick)

The umbrella `rustkick` crate re-exports everything in this crate plus
the core primitives, so app code can write a single `use rustkick::*;`
import. Depending on `rustkick-http` directly only makes sense if
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

## Quick example

```rust
use rustkick_http::{bootstrap, define_module, Inject, KickResult};
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
[`examples/users-api`](https://github.com/forinda/rustkick/tree/main/examples/users-api).

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
rustkick-http = "0.0"
```

## Status

Early — at `0.0.x`. API surface is reserved but may shift before
`v0.1.0`. See
[`RELEASE.md`](https://github.com/forinda/rustkick/blob/main/RELEASE.md)
for the versioning model.

## License

MIT — see the workspace root.
