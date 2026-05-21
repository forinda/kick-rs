# rustkick

> **A Rust port of [KickJS](https://github.com/forinda/kick-js).**
> Module-driven, adapter-extensible, contributor-pipelined web framework
> on [axum](https://github.com/tokio-rs/axum) + [tokio](https://tokio.rs).
> DB-agnostic. Compile-time DI. Typed context contributors.

[![status](https://img.shields.io/badge/status-phase--1--complete-green)](#status)
[![license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![rust](https://img.shields.io/badge/rust-1.78%2B-orange)](rust-toolchain.toml)

---

## Status

**Phase 1 done. The framework works end-to-end for the surface that's landed.**

What works right now:

- **Typed DI container** — three scopes (singleton, transient, request-stub),
  build-time duplicate detection, fast read-lock resolution
- **Module composition** — providers fold across modules and sub-modules into
  one container; cross-module conflicts caught at build
- **Adapter & Plugin factories** — `define_adapter()` / `define_plugin()` with
  `.call()` / `.with(cfg)` / `.scoped(name, cfg)` variants
- **Cooperative shutdown** — `tokio::join_all` across adapters with per-adapter
  timeout budget
- **`bootstrap().listen(addr)`** — real axum server with the full lifecycle:
  topo-sort adapters → `before_mount` → `before_start` → bind → `after_start` →
  serve with Ctrl-C graceful shutdown → `shutdown()`
- **`Inject<T>` extractor** — axum-native DI access in handlers, with structured
  errors (`RK_E_UNKNOWN_TOKEN`) returned as RFC 7807 problem-details JSON
- **`define_module(...)`** with `.get`/`.post`/`.put`/`.patch`/`.delete`, prefix
  application, and sub-module nesting

Build state:

- `cargo build --workspace` — clean
- `cargo test --workspace` — **45/45 passing**
- `cargo clippy --workspace --lib --tests -- -D warnings` — clean

What does **not** yet exist:

- `cargo rustkick` CLI — placeholder binary (Phase 5)
- `#[service]` / `#[handler]` / `#[get]` proc-macros — Phase 3
- Context contributors with typed `Deps` — Phase 4
- DB adapter (sqlx/diesel/sea-orm) — explicitly out of scope; lives in
  user code. See [`examples/users-api`](./examples/users-api) for the
  pattern adopters follow.

See [`SPEC.md`](./SPEC.md) for the design, [`ARCHITECTURE.md`](./ARCHITECTURE.md)
for internals, and the [phase roadmap](./SPEC.md#11-implementation-phases)
for what's next.

> If you depend on this before v0.1.0, pin to a specific git SHA. The
> API surface is reserved but the implementations haven't been
> battle-tested.

---

## Why a Rust port?

KickJS gave Node.js developers NestJS ergonomics without the complexity —
decorators, DI, modules, adapters, code generators, end-to-end type safety.
Rust developers have axum (excellent, low-level), actix-web (fast, mature),
or a handful of mid-level frameworks — but none with the **module +
adapter + contributor pipeline** model that makes large KickJS apps stay
organized as they grow.

Rustkick brings that model over, with the things Rust does better:

| KickJS                                  | rustkick                                            |
|-----------------------------------------|-----------------------------------------------------|
| Decorator metadata, runtime reflection  | Proc-macros, real types, compile-time wiring        |
| `kick typegen` to sync routes ↔ types   | Routes ↔ types are the same thing, always           |
| Boot-time contributor cycle check       | Compile-time check via typed tuple `Deps`           |
| `reflect-metadata` at startup           | Zero runtime metadata — proc-macros expand to code  |
| `Promise.allSettled` for shutdown       | `tokio::join!` + per-adapter timeout budgets        |
| `@Inject('app/users/repository')`       | `Inject<UserRepository>` extractor on the handler   |

See the [comparison table in SPEC.md §10](./SPEC.md#10-comparison-kickjs--rustkick)
for the full row-by-row mapping.

---

## Hello world (compiles today)

```rust
use rustkick::{bootstrap, define_module, Inject, KickResult};
use axum::Json;
use std::sync::Arc;

struct HelloService;
impl HelloService {
    fn greet(&self, name: &str) -> serde_json::Value {
        serde_json::json!({ "message": format!("Hello {name} from rustkick!") })
    }
}

async fn index(svc: Inject<HelloService>) -> Json<serde_json::Value> {
    Json(svc.greet("World"))
}

fn hello_module() -> rustkick::Module {
    define_module("hello")
        .prefix("/hello")
        .service_value(HelloService)
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

> The `#[service]` proc-macro that auto-wires `Inject<T>` fields lands in
> Phase 3 — until then `.service_value(value)` and `.service_factory(|c|
> Arc::new(...))` are the explicit equivalents.

---

## Roadmap

The full phase plan lives in [SPEC.md §11](./SPEC.md#11-implementation-phases).
Top-level summary:

| Phase | Goal                                                            | Status   |
|-------|-----------------------------------------------------------------|----------|
| 0     | Spec + architecture documents, workspace scaffold               | **Done** |
| 1     | `rustkick-core` Container/Module/Adapter + `rustkick-http` axum | **Done** |
| 2     | `examples/users-api`: CRUD with a local sqlx Postgres adapter   | **Done** |
| 3     | `rustkick-macros`: `#[service]` / `#[handler]` / `#[get]` sugar | Pending  |
| 4     | Context contributors with typed tuple `Deps`                    | Pending  |
| 5     | Adapter shutdown polish, OpenAPI, auth, CLI                     | Pending  |
| 6     | Ecosystem crates (ws, queue, otel, devtools)                    | Future   |

DB-related crates (`rustkick-pg`, `rustkick-diesel`, …) are **not** on the
roadmap. DB code lives in user code or examples; the framework stays lean.

---

## Crate layout

```
rust-pg/                          # this repo (will rename to rustkick before publish)
├── SPEC.md                       # design spec
├── ARCHITECTURE.md               # internals
├── README.md                     # this file
├── Cargo.toml                    # workspace manifest
└── crates/
    ├── rustkick/                 # umbrella crate — `use rustkick::*`
    ├── rustkick-core/            # Container, Module, Adapter, Plugin, errors
    ├── rustkick-http/            # axum integration, Inject, Ctx, bootstrap
    ├── rustkick-macros/          # #[service], #[handler], #[get] proc-macros
    ├── rustkick-config/          # env loader + ConfigService
    ├── rustkick-assets/          # asset manifest + typed keys
    └── rustkick-cli/             # `cargo rustkick` subcommand
```

`examples/` will appear once Phase 2 begins.

---

## Installing rustkick in your project

Rust packages ("crates") are distributed via three mechanisms — all
first-class in `cargo`. Pick whichever matches what's published today.

### 1. From this git repo (only option today)

```toml
[dependencies]
rustkick = { git = "https://github.com/forinda/rustkick", branch = "feature/foundation", features = ["macros"] }

# Pin to a specific commit for reproducibility (recommended pre-v0.1.0):
# rustkick = { git = "https://github.com/forinda/rustkick", rev = "<sha>", features = ["macros"] }
```

`cargo` natively resolves git dependencies — no extra registry config,
no auth required for public repos. For a private repo, set up
[git credentials](https://doc.rust-lang.org/cargo/reference/registries.html#authentication)
or use SSH URLs.

### 2. From crates.io (planned, post-v0.1.0)

```toml
[dependencies]
rustkick = { version = "0.1", features = ["macros"] }
```

This is the public Rust registry at <https://crates.io>. One account,
`cargo publish` once per release. Not yet — rustkick is pre-release.

### 3. From a local path (during framework development)

```toml
[dependencies]
rustkick = { path = "../rustkick/crates/rustkick", features = ["macros"] }
```

Useful when you're hacking on rustkick and a real app side by side.

---

## Development

Cargo lives at `~/.cargo/bin/cargo` on most setups. From the workspace
root:

```bash
# build everything
cargo build --workspace

# run the (4) passing tests
cargo test --workspace

# format
cargo fmt --all

# lint (recommended before commits)
cargo clippy --workspace --all-targets -- -D warnings
```

The recommended dev loop is `cargo watch -x 'build --workspace'` —
install with `cargo install cargo-watch`.

---

## Versioning (planned)

Each crate in `crates/` will version independently — same model as
`tokio-*` and `tower-*`. Release tags: `<crate>-vX.Y.Z`. The umbrella
`rustkick` crate will pin matching minor versions of `rustkick-core`
and `rustkick-http`.

Not in effect yet — everything is at `0.0.0` until Phase 1 lands.

---

## Contributing

The foundation is the priority right now. If you want to help:

1. Read [`SPEC.md`](./SPEC.md) and [`ARCHITECTURE.md`](./ARCHITECTURE.md).
2. Open an issue before non-trivial PRs — the design is still moving fast.
3. Each PR should reference a phase in the [roadmap](./SPEC.md#11-implementation-phases).

---

## License

MIT — see [LICENSE](./LICENSE).

---

## Acknowledgements

This project is a direct port of [KickJS](https://github.com/forinda/kick-js)
by [@forinda](https://github.com/forinda). The architectural decisions —
module system, adapter lifecycle, context contributor pipeline, mount
sort, factory variants — are all KickJS originals, translated into
Rust idioms where the type system can enforce what JavaScript could
only validate at boot.
