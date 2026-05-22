# rustkick

> A Rust port of [KickJS](https://github.com/forinda/kick-js). Module-driven,
> adapter-extensible, contributor-pipelined web framework on
> [axum](https://github.com/tokio-rs/axum) + [tokio](https://tokio.rs).
> DB-agnostic. Compile-time DI. Typed context contributors.

[![crates.io](https://img.shields.io/crates/v/rustkick.svg)](https://crates.io/crates/rustkick)
[![docs.rs](https://docs.rs/rustkick/badge.svg)](https://docs.rs/rustkick)
[![license](https://img.shields.io/crates/l/rustkick.svg)](https://github.com/forinda/rustkick/blob/main/LICENSE)

This is the **umbrella crate** — depend on `rustkick` and you get
`Container` / `Module` / `Adapter` / `Plugin` / `bootstrap()` /
`Inject<T>` / `HttpModule` re-exported under one prefix.

## Install

```toml
[dependencies]
rustkick = "0.0"
```

## Hello world

```rust
use rustkick::{bootstrap, define_module, Inject, KickResult};
use axum::Json;

struct HelloService;
impl HelloService {
    fn greet(&self, name: &str) -> serde_json::Value {
        serde_json::json!({ "message": format!("Hello {name} from rustkick!") })
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

A larger end-to-end example (users CRUD on Postgres) is at
[`examples/users-api`](https://github.com/forinda/rustkick/tree/main/examples/users-api).

## What this crate is

A thin re-export wrapper. The actual implementation lives in two
focused crates:

| Crate                                                     | What it owns                                          |
|-----------------------------------------------------------|-------------------------------------------------------|
| [`rustkick-core`](https://crates.io/crates/rustkick-core) | DI container, modules, adapters, plugins, error model |
| [`rustkick-http`](https://crates.io/crates/rustkick-http) | axum integration: bootstrap, Inject, HttpModule       |

When `define_module` is referenced via the umbrella, the HTTP variant
wins by default (since that's what app authors compose against). The
transport-agnostic core variant remains reachable as
`rustkick::CoreModule` / `rustkick::CoreModuleBuilder`.

## When to depend on a sub-crate directly

Almost never — `rustkick` is the path of least surprise. The two cases
where bypassing the umbrella makes sense:

- **Non-HTTP applications** (queue workers, CLIs) — depend on
  `rustkick-core` only and skip the axum/tokio HTTP machinery.
- **Library authors** publishing rustkick adapters/plugins — depend on
  the specific layer your code touches rather than pulling the whole
  umbrella into your downstream consumers.

## Coming back later

The umbrella will gain three more re-exports as their underlying
crates reach publish-ready state:

| Feature       | Underlying crate          | Lands in |
|---------------|---------------------------|----------|
| `#[service]` / `#[handler]` / `#[get]` macros | `rustkick-macros`  | Phase 3  |
| Env-driven config + `ConfigService`           | `rustkick-config`  | Phase 5  |
| Typed asset manifest                           | `rustkick-assets`  | Phase 5  |

They'll appear as optional features once each crate is real. Until
then they aren't part of the umbrella surface — see the workspace
[`SPEC.md`](https://github.com/forinda/rustkick/blob/main/SPEC.md)
for the roadmap.

## Status

Early — at `0.0.x`. API surface is reserved but may shift before
`v0.1.0`. See
[`RELEASE.md`](https://github.com/forinda/rustkick/blob/main/RELEASE.md)
for the versioning model.

## License

MIT — see the workspace root.
