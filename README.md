# kick-rs

> **A Rust port of [KickJS](https://github.com/forinda/kick-js).**
> Module-driven web framework on [axum](https://github.com/tokio-rs/axum) +
> [tokio](https://tokio.rs). Compile-time DI, typed context
> contributors, opt-in OpenAPI, and a `cargo kick` CLI that scaffolds,
> generates, watches, and lints.

[![crates.io](https://img.shields.io/crates/v/kick-rs.svg)](https://crates.io/crates/kick-rs)
[![docs.rs](https://docs.rs/kick-rs/badge.svg)](https://docs.rs/kick-rs)
[![license](https://img.shields.io/crates/l/kick-rs.svg)](LICENSE)
[![rust](https://img.shields.io/badge/rust-1.78%2B-orange)](rust-toolchain.toml)

---

## Quickstart — 60 seconds

```bash
# 1. install the CLI
cargo install kick-rs-cli

# 2. scaffold a new app
cargo kick new my-app && cd my-app

# 3. run it
cargo run

# 4. hit it
curl http://localhost:3000/hello           # {"message":"Hello from kick-rs!"}
curl http://localhost:3000/hello/world     # {"message":"Hello, world!"}
```

A working `bootstrap()` chain + one module + a hello-world handler.
From there:

```bash
cargo kick g module posts                  # adds src/modules/posts/ and registers it
cargo kick g service posts/post_service    # adds a #[service] stub + .service::<>() call
cargo kick add openapi                     # toggles the openapi feature on Cargo.toml
cargo kick dev                             # watch + restart on save (Ctrl-C to quit)
cargo kick info                            # snapshot of modules + services + features
cargo kick check                           # lint for unmounted modules / unregistered services
```

The whole CLI surface is documented at
[`crates/kick-rs-cli/README.md`](./crates/kick-rs-cli/README.md).

---

## What you get

| Area               | What ships                                                                 |
|--------------------|----------------------------------------------------------------------------|
| **DI**             | Compile-time container, three scopes (singleton / transient / request); `Inject<T>` extractor |
| **Modules**        | `define_module()` composes routes + services + sub-modules; conditional mount via `Bootstrap::setup` |
| **Adapters**       | Full lifecycle (`before_mount` → `before_start` → `after_start` → cooperative `shutdown`) |
| **Plugins**        | Ship modules, adapters, contributors, lifecycle hooks, phase-keyword middleware as one unit |
| **Contributors**   | Typed `Deps`, topo-sorted at boot, five registration sites, `OnErrorAction::{Propagate,Skip,Recover}` |
| **HTTP plugins**   | request-id, request-logger, CORS, compression, helmet, trace-context, OpenAPI, DevTools, Assets |
| **OpenAPI**        | `#[utoipa::path(...)]` + `paths!(...)` macro auto-collects spec; served at `/openapi.json` |
| **Config**         | Layered loader: defaults → TOML → JSON → dotenv → env. Typed extraction via serde |
| **Assets**         | `embed_assets!` bundles a `dist/` tree at compile time; `AssetsPlugin` serves with immutable cache headers |
| **DevTools**       | `GET /__debug` returns a JSON snapshot of modules / plugins / adapters / contributors |
| **CLI**            | `new`, `g {module,service,contributor}`, `add`, `info`, `dev`, `check`     |

DB code is **deliberately not in the framework** — examples show
the canonical patterns (`sqlx` in `users-api`, schema-per-tenant pools
in `multi-tenant-api`), but adopters pick their own ORM / driver.

---

## Anatomy of a kick-rs app

```rust,ignore
use kick_rs::{bootstrap, define_module, service, Inject, KickResult, Module};
use axum::Json;

#[service]
pub struct HelloService;

impl HelloService {
    fn greet(&self, name: &str) -> serde_json::Value {
        serde_json::json!({ "message": format!("Hello {name}!") })
    }
}

async fn index(svc: Inject<HelloService>) -> Json<serde_json::Value> {
    Json(svc.greet("World"))
}

fn hello_module() -> Module {
    define_module("hello")
        .prefix("/hello")
        .service::<HelloService>()
        .get("/", index)
        .build()
}

#[tokio::main]
async fn main() -> KickResult<()> {
    bootstrap()
        .module(hello_module())
        .listen("0.0.0.0:3000")
        .await
}
```

`cargo kick new` emits exactly this shape; `cargo kick g service`
adds new `#[service]` structs to the chain; `cargo kick g contributor`
adds `#[contributor]` async fns to the contributor pipeline.

---

## Context contributors — the distinctive piece

A `ContextContributor` declares a *per-request* value with typed
dependencies on other contributors. The pipeline runs once per
request, topo-sorted, so each `Deps` tuple sees `&T` to upstream
values. Missing producers and cycles fail at **boot**, not at
request time.

```rust,ignore
struct LoadTenant;
impl ContextContributor for LoadTenant {
    type Key = Tenant;
    type Deps = ();
    async fn resolve<'a>(&'a self, _: &'a dyn ContributorRequest, _: ()) -> KickResult<Tenant> {
        // extract from headers / JWT / etc.
        Ok(Tenant { slug: "acme".into() })
    }
}

struct LoadTenantDb;
impl ContextContributor for LoadTenantDb {
    type Key = TenantDb;
    type Deps = (Tenant,);  // typed dep — pipeline orders us after LoadTenant
    async fn resolve<'a>(&'a self, _: &'a dyn ContributorRequest, (t,): (&'a Tenant,)) -> KickResult<TenantDb> {
        Ok(TenantDb::for_tenant(&t.slug).await?)
    }
}

bootstrap()
    .contribute(LoadTenant)
    .contribute(LoadTenantDb)
    .module(posts_module())
    .listen("0.0.0.0:3000").await
```

Handlers pull contributor outputs via `Ctx<T>`:

```rust,ignore
async fn show(tenant: Ctx<Tenant>, db: Ctx<TenantDb>) -> Json<Stats> { /* ... */ }
```

This is how per-tenant DB instantiation, current-user resolution,
request-id propagation fall out as plain typed code — no
framework-internal magic, no string lookups.

The `multi-tenant-api` example wires this end-to-end; see
[`examples/multi-tenant-api/`](./examples/multi-tenant-api/).

---

## Examples

| Example                                                            | What it shows                                                                            |
|--------------------------------------------------------------------|------------------------------------------------------------------------------------------|
| [`examples/users-api/`](./examples/users-api/)                     | Postgres CRUD via sqlx; auto-collected OpenAPI spec at `/openapi.json`; embedded `dist/` static assets at `/static/*` |
| [`examples/multi-tenant-api/`](./examples/multi-tenant-api/)       | Schema-per-tenant pools via `LoadTenant`/`LoadTenantDb` contributors; `x-tenant-slug` header routes to the right schema |

Both load config via `kick_rs::config::Config::builder()` and serve
their OpenAPI spec; users-api also bundles a tiny static landing
page via `embed_assets!`.

---

## Crate layout

| Crate                     | What it ships                                                                  |
|---------------------------|--------------------------------------------------------------------------------|
| `kick-rs`                 | Umbrella — single `use kick_rs::*;` for adopter code                           |
| `kick-rs-core`            | DI Container, modules, adapters, plugins, context contributors, error model    |
| `kick-rs-http`            | axum integration: `bootstrap`, `Inject`, `Ctx`, route macros, built-in plugins |
| `kick-rs-macros`          | `#[service]`, `#[contributor]`, `#[get]/#[post]/...`, `paths!(...)`            |
| `kick-rs-config`          | Layered env / dotenv / TOML / JSON config loader                              |
| `kick-rs-assets`          | `AssetManifest` + `embed_assets!` runtime types                                |
| `kick-rs-assets-macros`   | Proc-macro half of assets (consumed transitively)                              |
| `kick-rs-cli`             | `cargo-kick` binary — scaffold / generate / dev / check                        |

All published independently on crates.io; the umbrella `kick-rs`
pins matching versions so adopters need exactly one dep line.

---

## Adding kick-rs to an existing project

```toml
[dependencies]
kick-rs = "0.1.0-alpha.6"

# enable the features you want:
kick-rs = { version = "0.1.0-alpha.6", features = [
    "macros",      # #[service], #[contributor], #[get]/#[post]/...
    "config",      # layered Config::builder()
    "openapi",     # OpenApiPlugin + paths!()
    "devtools",    # /__debug introspection
    "assets",      # embed_assets! + AssetsPlugin
] }
```

> Cargo doesn't auto-select pre-release versions from a range like
> `kick-rs = "0.1"` — until a stable `0.1.0` ships, spell out the
> full `0.1.0-alpha.6`.

Most adopters scaffold a fresh project with `cargo kick new <name>`
instead — the generated `Cargo.toml` already lists the right deps
and the most common feature set.

---

## Status

| Crate                    | Latest        |
|--------------------------|---------------|
| `kick-rs`                | `0.1.0-alpha.6` |
| `kick-rs-core`           | `0.1.0-alpha.4` |
| `kick-rs-http`           | `0.1.0-alpha.4` |
| `kick-rs-macros`         | `0.1.0-alpha.4` |
| `kick-rs-config`         | `0.1.0-alpha.4` |
| `kick-rs-assets`         | `0.1.0-alpha.3` |
| `kick-rs-assets-macros`  | `0.1.0-alpha.2` |
| `kick-rs-cli`            | `0.1.0-alpha.4` |

API surface is functionally complete and the alpha lane is open
for adopters. The framework, CLI, and both example apps are exercised
by ~250 tests + clippy `-D warnings` on every PR. Release cadence is
release-plz on every merge.

The path to `0.1.0` (no alpha) is one or two cycles of adopter
feedback — the surface itself is stable.

DB-related crates (`kick-rs-pg`, etc.) are **not** on the roadmap.
DB code lives in user code; the framework stays lean.

---

## Versioning

Each crate versions independently, same model as `tokio-*` /
`tower-*`. Release tags: `<crate>-vX.Y.Z`. The umbrella `kick-rs`
crate's `workspace.dependencies` pins matching minor versions of
the others; release-plz keeps everything in lockstep.

---

## Contributing

1. Read [`SPEC.md`](./SPEC.md) and [`ARCHITECTURE.md`](./ARCHITECTURE.md)
   for the design overview.
2. Pick something from the issue tracker or open one for discussion.
3. PRs should keep `cargo test --workspace --all-features` and
   `cargo clippy --workspace --all-features --all-targets -- -D warnings`
   green.

The release pipeline handles versioning + CHANGELOG entries
automatically — no manual version bumps needed.

---

## License

MIT — see [LICENSE](./LICENSE).

---

## Acknowledgements

This project is a direct port of [KickJS](https://github.com/forinda/kick-js)
by [@forinda](https://github.com/forinda). Module system, adapter
lifecycle, context contributor pipeline, mount sort, factory
variants — all KickJS originals, translated into Rust idioms.
