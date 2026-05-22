# rustkick — Design Specification

> A decorator-light, module-driven web framework for Rust, built on
> [axum](https://github.com/tokio-rs/axum) and [tokio](https://tokio.rs).
> Borrows the ergonomics of KickJS (Node.js) — modules, typed DI, context
> contributors, adapters — and translates them into idiomatic Rust where
> the type system can enforce what KickJS can only validate at boot.

**Status:** Draft 0.1 — pre-implementation specification
**Inspiration:** [KickJS](https://github.com/forinda/kick-js)

---

## Table of Contents

1. [Goals & Non-Goals](#1-goals--non-goals)
2. [Design Principles](#2-design-principles)
3. [Crate Layout](#3-crate-layout)
4. [Core Concepts](#4-core-concepts)
5. [Plugins (Deep Dive)](#5-plugins-deep-dive)
6. [Assets](#6-assets)
7. [CLI (`cargo rustkick`)](#7-cli-cargo-rustkick)
8. [Public API Surface](#8-public-api-surface)
9. [Sample: Users CRUD with Postgres](#9-sample-users-crud-with-postgres)
10. [Comparison: KickJS → rustkick](#10-comparison-kickjs--rustkick)
11. [Implementation Phases](#11-implementation-phases)
12. [Open Questions](#12-open-questions)

---

## 1. Goals & Non-Goals

### Goals

- **Module-driven architecture** — `define_module()` is the unit of composition.
  A module owns routes, services, and (optionally) adapters/plugins.
- **Typed DI container** — `Inject<T>` extractor with three scopes
  (singleton, request, transient). Token resolution errors surface at startup,
  not at first request.
- **Context contributors** — declarative, dependency-ordered per-request value
  population. Topologically sorted at boot; cycles fail compilation-or-startup.
- **Adapter & plugin factories** — `define_adapter()` / `define_plugin()`
  symmetric to KickJS, the same surface third-party crates use.
- **Axum-native** — handlers are plain `async fn`s with `axum::extract`
  semantics. `Inject<T>` is just another `FromRequestParts` extractor.
- **Optional sugar via macros** — `#[service]`, `#[handler]`, `#[get("/")]`
  are opt-in. The bare builder path is the source of truth.
- **Database-agnostic core** — no DB code in framework crates. Adopters
  bring their own (sqlx, diesel, sea-orm, custom) and wire it via a normal
  adapter. Examples demonstrate this with sqlx + Postgres.
- **Cooperative shutdown** — `tokio::join!` on adapter shutdown; observability
  SDKs can own SIGTERM independently via a configurable `process_hooks` mode.

### Non-Goals (v0.1)

- **Decorator/Reflection runtime** — no `reflect-metadata` analog. Wiring is
  builder calls + proc macros that expand to those calls.
- **ORM / database driver** — entirely out of scope. The core stays lean
  and DB-agnostic; the example wires sqlx itself as a local adapter to
  demonstrate the pattern.
- **Hot module reload** — `cargo watch` is the dev loop. No
  in-process module swapping.
- **Multi-runtime support** — tokio only for v0.1. `async-std` / `smol`
  abstractions are deferred.
- **GraphQL / WS / Queues** — out of scope for v0.1; designed to land as
  optional adapter crates later (mirror KickJS `kick add` model).

---

## 2. Design Principles

| # | Principle | Implication |
|---|-----------|-------------|
| 1 | **Builder is canonical, macros are sugar** | Every macro expands to a builder call you could write yourself. No magic. |
| 2 | **Errors at boot, not on the hot path** | Missing DI tokens, cyclic contributors, unmounted modules all fail `bootstrap()` with a structured `KickError`. |
| 3 | **Small stable core + opt-in adapters** | `rustkick` (core), `rustkick-http`, `rustkick-pg` are baseline. Auth/OpenAPI/WS/etc. ship as separate crates with feature flags. |
| 4 | **Co-location over central config** | Routes, services, and contributors live next to the module they belong to. Wiring uses `define_module().mount(...)`. |
| 5 | **Trait objects only where dynamic dispatch is genuinely needed** | DI container is the boundary. Inside a handler, `Inject<T>` returns the concrete type (or its `Arc`), not `dyn Trait`. |
| 6 | **Async-first, sync-second** | Every adapter hook is `async fn`. Sync code wraps in `spawn_blocking` if needed. |
| 7 | **Match axum idioms, don't replace them** | We add `Inject<T>`, `Ctx<T>`, and routing helpers. `Json`, `Path`, `Query`, `State` from axum continue to work unchanged. |

---

## 3. Crate Layout

```
rust-pg/                                  # workspace root
├── Cargo.toml                            # workspace manifest
├── SPEC.md                               # this file
├── ARCHITECTURE.md                       # internals deep-dive
├── README.md
├── rust-toolchain.toml                   # pinned toolchain
├── crates/
│   ├── rustkick/                         # umbrella crate, re-exports core+http
│   ├── rustkick-core/                    # Container, Module, Adapter, Plugin, errors
│   ├── rustkick-http/                    # axum integration, bootstrap, Inject extractor
│   ├── rustkick-macros/                  # #[service] / #[handler] / #[get] / #[plugin] proc-macros
│   ├── rustkick-config/                  # env loading + ConfigService
│   ├── rustkick-assets/                  # typed asset manifest + cache-busting resolver
│   └── rustkick-cli/                     # `cargo rustkick` subcommand (new, dev, g, add)
└── examples/
    └── users-api/                        # Users CRUD on Postgres (sqlx — example-local, not a framework crate)
```

### Crate dependency graph

```
                 ┌──────────────┐
                 │  rustkick    │ ◄── apps depend on this
                 │  (umbrella)  │
                 └──────┬───────┘
                        │ re-exports
        ┌───────────────┼─────────────────┐
        ▼               ▼                 ▼
┌───────────────┐ ┌──────────────┐ ┌────────────────┐
│ rustkick-core │ │ rustkick-http│ │ rustkick-macros│
└───────────────┘ └──────┬───────┘ └────────┬───────┘
       ▲                 │                  │
       └─ depended on by ┴──────────────────┘

  Optional add-on crates (a la carte, no DB anywhere in this list):

  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
  │ rustkick-config  │  │ rustkick-assets  │  │  rustkick-cli    │
  │  (env loader)    │  │ (manifest+keys)  │  │ (cargo subcmd)   │
  └──────────────────┘  └──────────────────┘  └──────────────────┘
```

### Why the umbrella crate

KickJS publishes `@forinda/kickjs` as a single import for app code. We mirror
that: app authors put **one** dependency in `Cargo.toml` and get
`rustkick::{Container, Module, bootstrap, Inject, …}`. Adapter crates can
still be added a la carte.

```toml
[dependencies]
# From crates.io:
rustkick = "0.0"

# From git for unreleased work:
# rustkick = { git = "https://github.com/forinda/rustkick", branch = "main" }

# From a local path during dev:
# rustkick = { path = "../rustkick/crates/rustkick" }
```

> The `macros` / `config` / `assets` features will return as optional
> deps on the umbrella once those crates become publishable
> (Phase 3 / Phase 5). Until then, depend on the auxiliary crate
> directly via git if you need it pre-publish.

---

## 4. Core Concepts

### 4.1 Container

Holds typed providers keyed by `TypeId` (or named `Token<T>`). Three scopes:

| Scope | Lifetime | Storage |
|-------|----------|---------|
| `Singleton` | Process lifetime | `Arc<T>` held by container |
| `RequestScoped` | One request | Stored on `http::Extensions`, dropped at response |
| `Transient` | One resolution | Factory closure called every `Inject<T>` |

```rust
let container = Container::builder()
    .singleton::<PgPool>(pool)
    .singleton::<UserService>()                          // auto-wired
    .request_scoped::<RequestUser>(|ctx| extract_jwt(ctx))
    .transient::<UuidGen>(|| UuidGen::new_v7())
    .build()?;                                           // ← Result: missing deps fail here
```

Resolution order: **request-scoped → singleton → transient**. Missing tokens
produce `KickError::UnknownToken { token, expected_at }`.

### 4.2 Token

For named or trait-object bindings (where TypeId alone isn't enough):

```rust
let USER_REPO: Token<dyn UserRepo> = Token::new("users/repository");

container.bind(USER_REPO, Arc::new(PgUserRepo::new(pool)));
// In handler:
async fn list(repo: Inject<USER_REPO>) -> Json<Vec<User>> { ... }
```

Tokens carry intent into errors: `KickError::UnknownToken { token: "users/repository", … }`.

### 4.3 Module

```rust
pub fn users_module() -> Module {
    define_module("users")
        .prefix("/users")
        .service::<UserService>()
        .service::<UserRepository>()
        .get("/", handlers::list)
        .get("/:id", handlers::show)
        .post("/", handlers::create)
        .patch("/:id", handlers::update)
        .delete("/:id", handlers::delete)
        .contribute::<LoadCurrentUser>()                 // request contributor
        .build()
}
```

Module is a value, not a trait — composes via `define_modules().mount(users_module())`.

### 4.4 Adapter

Adapters are long-lived components with lifecycle hooks. Examples: Postgres
pool, OpenTelemetry exporter, Redis client, WebSocket hub.

```rust
#[async_trait]
pub trait Adapter: Send + Sync + 'static {
    fn name(&self) -> &str;
    async fn before_mount(&self, ctx: &AdapterContext) -> KickResult<()> { Ok(()) }
    async fn before_start(&self, ctx: &AdapterContext) -> KickResult<()> { Ok(()) }
    async fn after_start (&self, ctx: &AdapterContext) -> KickResult<()> { Ok(()) }
    async fn shutdown    (&self) -> KickResult<()> { Ok(()) }
    fn layers(&self) -> Vec<RouterLayer> { vec![] }     // tower layers to apply
    fn depends_on(&self) -> &[&str] { &[] }
}
```

Built with `define_adapter()`:

```rust
let pg_adapter = define_adapter::<PgConfig, _>("postgres")
    .defaults(PgConfig { max_conns: 10, ..Default::default() })
    .build(|cfg, ctx| async move {
        let pool = PgPoolOptions::new()
            .max_connections(cfg.max_conns)
            .connect(&cfg.url).await?;
        ctx.container.bind_singleton(pool);
        Ok(())
    });
```

### 4.5 Plugin

Plugin = adapter without lifecycle (pure DI registrations + middleware +
contributors). Same factory shape, sorted via `depends_on` at boot.
See [§5](#5-plugins-deep-dive) for the full surface.

### 4.6 Context Contributor

The most novel pattern carried over from KickJS — populates `Ctx::get::<T>()`
declaratively, with explicit dependencies, topo-sorted once at boot.

**Five registration sites, precedence high → low: method > handler-class >
module > adapter/plugin > global.** Higher-precedence contributors with the
same `Key` override lower ones; this is the same matrix KickJS uses for
`@LoadTenant` vs module-level vs `bootstrap({ contributors })`.

```rust
pub struct LoadTenant;

#[async_trait]
impl ContextContributor for LoadTenant {
    type Key = Tenant;
    type Deps = ();
    async fn resolve(&self, ctx: &RequestContext, _: ()) -> KickResult<Tenant> {
        let tenant_id = ctx.headers().get("x-tenant-id")
            .ok_or(KickError::missing_header("x-tenant-id"))?;
        tenants::find_by_id(ctx.inject::<PgPool>(), tenant_id).await
    }
}

pub struct LoadProject;

#[async_trait]
impl ContextContributor for LoadProject {
    type Key = Project;
    type Deps = (Tenant,);                               // ← typed dependency
    async fn resolve(&self, ctx: &RequestContext, (tenant,): (Tenant,)) -> KickResult<Project> {
        projects::find(&tenant.id, ctx.path_param("id")?).await
    }
}
```

Compile-time guarantee: if you remove `LoadTenant` from the module but
`LoadProject` declares it as a dep, **the program does not type-check**.
This is stricter than KickJS, which only catches it at boot via
`MissingContributorError`.

Inside a handler:

```rust
async fn show_project(ctx: Ctx) -> Json<Project> {
    Json(ctx.get::<Project>().clone())                   // guaranteed populated
}
```

### 4.6.1 Middleware entries (path-scoped)

User middleware accepted by `bootstrap().middleware(…)` is either a bare
tower `Layer` (applied globally) or a `MiddlewareEntry { path, layer }`
for path-scoped application:

```rust
bootstrap()
    .middleware(tower_http::cors::CorsLayer::permissive())                     // global
    .middleware(MiddlewareEntry::at("/admin", BasicAuthLayer::new(creds)))     // scoped
```

### 4.7 RequestContext (`Ctx`)

Wraps `axum::http::Request` + a typed key-value store + container reference.

```rust
pub struct Ctx<P = ()> {
    pub params: P,                                       // typed path params
    pub headers: HeaderMap,
    pub request_id: RequestId,
    container: Arc<Container>,
    extensions: Extensions,                              // contributor outputs live here
}

impl<P> Ctx<P> {
    pub fn get<T: 'static>(&self) -> &T { ... }
    pub fn try_get<T: 'static>(&self) -> Option<&T> { ... }
    pub fn inject<T: 'static>(&self) -> Arc<T> { ... }
    pub async fn body_json<T: DeserializeOwned>(&mut self) -> KickResult<T> { ... }
    pub fn paginate<T>(&self, items: Vec<T>, total: u64) -> Paginated<T> { ... }
}

/// RFC-aligned paginated response shape — same JSON wire format as KickJS.
pub struct Paginated<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub total_pages: u32,
}
```

`Ctx<P>` derives the param type from the route at compile time when used
with the `#[get("/users/:id")]` sugar. Without sugar, the user types it.

### 4.8 Errors — `KickError`

All framework errors are structured:

```rust
pub struct KickError {
    pub code: &'static str,                              // "RK_E_UNKNOWN_TOKEN"
    pub message: String,
    pub fix_hint: Option<String>,
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
    pub context: BTreeMap<String, String>,
}
```

Same shape as KickJS `KickError`. Display format includes the fix hint
so users see actionable messages, not stack traces.

### 4.9 Factory variants — `.scoped()` and `.async_()`

All three factories (`define_module`, `define_adapter`, `define_plugin`)
share a uniform shape — call form, `.scoped()`, and `.async_()` — so
adopters learn one mental model.

| Variant | Use case |
|---------|----------|
| `factory(config)`              | Single instance with eager config |
| `factory.scoped(name, config)` | Multi-instance — namespaces the registered name + DI tokens (e.g., `"postgres:reads"` vs `"postgres:writes"`) |
| `factory.async_(opts)`         | Config resolved during `before_start` from injected sources (secrets store, remote config) |

### 4.10 Module Registry (dynamic mount)

`define_modules()` accepts both static `.mount(my_module())` and a
`.setup(|reg| { … })` form for conditional registration:

```rust
define_modules()
    .mount(users_module())
    .setup(|reg| {
        if env.feature_billing {
            reg.mount(billing_module());
        }
    })
```

Used for feature flags and tenant-conditional modules, mirroring KickJS
`MutableModuleRegistry`.

### 4.11 Introspection (DevTools contract)

Adapters and plugins may implement an optional `introspect()` method
returning a typed snapshot. Built-in `/__debug` endpoint
(feature-gated, off by default) reads these to render the topology
graph, DI registry, route table, and adapter state as JSON.

```rust
pub trait Introspect {
    fn introspect(&self) -> IntrospectionSnapshot;
}

pub struct IntrospectionSnapshot {
    pub kind: IntrospectionKind,            // Adapter | Plugin | Module
    pub name: String,
    pub state: serde_json::Value,           // free-form per-component
    pub tokens: Vec<String>,
    pub memory_bytes: Option<usize>,
}
```

Identical contract to KickJS `introspect()` — same protocol version field,
same `kind` enum.

### 4.12 Container change events

Every singleton `bind`, transient resolve, and request-scoped insert
emits a batched event (50ms debounce) so observers (DevTools, hot-reload
listeners, swagger regenerator) can react without polling:

```rust
container.on_change(|events| {
    for e in events { tracing::trace!(?e, "container change"); }
});
```

Off the hot path — debouncing means handler-time resolves don't allocate
per resolve.

### 4.13 Scope validation

The builder rejects DI graphs where a `Singleton` provider depends on a
`RequestScoped` one — the singleton outlives any request, so its
constructor cannot legally close over a per-request value. Surfaces as
`RK_E_INVALID_SCOPE { singleton, depends_on_request }` at `Container::build()`.

### 4.14 Bootstrap

```rust
#[tokio::main]
async fn main() -> KickResult<()> {
    rustkick::bootstrap()
        .modules(modules())
        .adapter(pg_adapter)
        .adapter(otel_adapter)
        .middleware(tower_http::trace::TraceLayer::new_for_http())
        .listen("0.0.0.0:3000")
        .await
}
```

Flow:
1. Build `Container` from all module/adapter providers (singleton phase).
2. Run `Adapter::before_mount` for each, topo-sorted by `depends_on`.
3. Build `axum::Router` from all modules.
4. Wrap router with adapter layers + user middleware.
5. Run `Adapter::before_start`, then `axum::serve`.
6. Run `Adapter::after_start`.
7. On SIGTERM: `tokio::join!` all `Adapter::shutdown`, with a configurable timeout.

---

## 5. Plugins (Deep Dive)

### 5.1 What a plugin is

A **plugin** is a packaged bundle of: DI providers, tower layers, context
contributors, and route fragments — assembled by a factory, registered by
name, sorted by `depends_on`, and validated at boot. It is the
*third-party extension point*: the same surface rustkick itself uses
internally for first-party features like request-id and the request
logger.

```
┌─────────────────────────────────────────────┐
│                  Plugin                     │
│  ┌──────────────┐   ┌──────────────────┐    │
│  │ DI providers │   │  Tower layers    │    │
│  │              │   │                  │    │
│  └──────────────┘   └──────────────────┘    │
│  ┌──────────────┐   ┌──────────────────┐    │
│  │ Contributors │   │ Route fragments  │    │
│  │              │   │                  │    │
│  └──────────────┘   └──────────────────┘    │
│  name, depends_on, version                  │
└─────────────────────────────────────────────┘
```

### 5.2 Plugin trait

```rust
#[async_trait]
pub trait Plugin: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn version(&self) -> &str { env!("CARGO_PKG_VERSION") }
    fn depends_on(&self) -> &[&str] { &[] }

    /// Register DI providers. Runs during Container::build, before adapters.
    fn register(&self, builder: &mut ContainerBuilder) -> KickResult<()> { Ok(()) }

    /// Tower layers to apply at the router level.
    fn layers(&self) -> Vec<RouterLayer> { vec![] }

    /// Context contributors to add to the pipeline.
    fn contributors(&self) -> Vec<Box<dyn AnyContributor>> { vec![] }

    /// Optional route fragments mounted into the app router.
    fn routes(&self) -> Vec<RouteSpec> { vec![] }
}
```

### 5.3 `define_plugin()` factory

The canonical authoring shape — same ergonomics as `define_adapter()`:

```rust
pub fn analytics_plugin() -> PluginFactory<AnalyticsConfig> {
    define_plugin::<AnalyticsConfig, _>("analytics")
        .depends_on(&["request-id"])                     // sorted after request-id
        .defaults(AnalyticsConfig { sample_rate: 1.0 })
        .build(|cfg, ctx| async move {
            // ctx.container is the partial container — only earlier plugins/adapters
            // have registered at this point
            let client = AnalyticsClient::new(cfg.api_key.clone());
            ctx.container.bind_singleton(client);
            Ok(Plugin {
                layers: vec![RouterLayer::new(AnalyticsLayer::new(cfg.sample_rate))],
                contributors: vec![Box::new(AttachSessionId)],
                routes: vec![],
            })
        })
}
```

Calling the factory yields a configured plugin instance:

```rust
let analytics = analytics_plugin()(AnalyticsConfig {
    api_key: env.analytics_key.clone(),
    sample_rate: 0.1,
});

bootstrap()
    .plugin(analytics)
    // …
```

### 5.4 `.scoped()` and `.async_()` variants (KickJS parity)

```rust
// Two independent analytics clients, namespaced — useful for multi-tenant
let analytics_us = analytics_plugin().scoped("us", us_config);
let analytics_eu = analytics_plugin().scoped("eu", eu_config);

bootstrap()
    .plugin(analytics_us)
    .plugin(analytics_eu)
    // ...

// Async config resolution — useful when config comes from a secrets store
let secret_plugin = analytics_plugin().async_(|inject| async move {
    let secrets: Arc<SecretsClient> = inject(()).await;
    AnalyticsConfig { api_key: secrets.get("ANALYTICS_KEY").await?, sample_rate: 1.0 }
});
```

`.scoped(name)` namespaces the plugin's `name()` to `"analytics:us"` so two
instances coexist. `.async_()` resolves config during `before_start`.

### 5.5 Sort & validation

Same Kahn topo-sort as adapters (see ARCHITECTURE §4.2). Boot fails with:

- `RK_E_DUPLICATE_MOUNT` — two plugins with same `name()`
- `RK_E_MISSING_MOUNT_DEP` — `depends_on` references unknown name
- `RK_E_MOUNT_CYCLE` — circular `depends_on`

### 5.6 Augmentation registry (typed plugin presence)

Some user code wants to know *whether a plugin is mounted* — e.g.,
"only attach Sentry headers if `sentry` plugin is present." KickJS solves
this with `defineAugmentation()` extending `KickJsPluginRegistry`. Rust
analog uses a trait + feature flag:

```rust
// Inside rustkick-sentry crate
pub trait SentryAugmentation { fn sentry_client(&self) -> Arc<SentryClient>; }
impl SentryAugmentation for Container { /* … */ }

// In user code (only compiles if rustkick-sentry is a dep):
let client = container.sentry_client();
```

Plugin authors are free to expose extension traits on `Container` /
`Module` — this is the idiomatic Rust replacement for KickJS module
augmentation.

### 5.7 Built-in plugins shipped with rustkick

| Plugin           | What it adds                                                     |
|------------------|------------------------------------------------------------------|
| `request_id`     | Generates/propagates `X-Request-Id`; binds `RequestId` singleton |
| `request_logger` | tracing-based per-request log line (method, path, status, μs)    |
| `cors`           | Wraps `tower_http::cors`; reads config from `CorsConfig` token   |
| `helmet`         | Security headers (X-Frame-Options, HSTS, CSP)                    |
| `compression`    | gzip/br via `tower_http::compression`                            |
| `trace_context`  | W3C `traceparent` parsing → `TraceContext` token                 |

All built-in plugins live in `rustkick-http::plugins` and are feature-gated.

### 5.8 Plugin vs Adapter — when to pick which

| Use a **Plugin** when…                              | Use an **Adapter** when…                          |
|-----------------------------------------------------|---------------------------------------------------|
| The component is stateless or builds lazily         | The component owns a long-lived connection/pool   |
| You only need DI bindings + middleware              | You need `before_start` / `after_start` hooks     |
| Shutdown is a no-op                                 | You need to drain/flush on shutdown               |
| Examples: cors, helmet, request-id, analytics layer | Examples: postgres pool, otel exporter, ws hub    |

---

## 6. Assets

### 6.1 Why we need this

Rustkick is a server framework, but server-rendered HTML, email templates,
and admin UIs still need to point at hashed asset URLs that change every
build. KickJS solves this with `@Asset()` + a typed `assets.foo.bar`
manifest. We carry over the typed-key idea — without the codegen step.

### 6.2 Manifest shape

```json
// dist/assets-manifest.json (produced by build pipeline)
{
  "version": "1",
  "assets": {
    "app.js":       "/static/app.a1b2c3.js",
    "app.css":      "/static/app.d4e5f6.css",
    "logo.svg":     "/static/logo.7g8h9i.svg",
    "img/hero.png": "/static/img/hero.j0k1l2.png"
  }
}
```

### 6.3 Loading + resolving

```rust
use rustkick_assets::AssetManifest;

let manifest = AssetManifest::load("dist/assets-manifest.json")?;
container.bind_singleton(manifest);

// In any handler:
async fn home(assets: Inject<AssetManifest>) -> Html<String> {
    Html(format!(
        r#"<link rel="stylesheet" href="{}"><script src="{}"></script>"#,
        assets.resolve("app.css")?,
        assets.resolve("app.js")?,
    ))
}
```

`AssetManifest::resolve` returns `KickResult<&str>` — on miss, it yields
`KickError::UnknownAsset { key, available_keys }`.

### 6.4 Typed keys via macro

```rust
use rustkick_assets::asset_keys;

asset_keys! {
    pub mod ui {
        css { "app.css" }
        js  { "app.js" }
        img {
            "logo.svg",
            "img/hero.png",
        }
    }
}

// Now compile-time-checked:
async fn home(assets: Inject<AssetManifest>) -> Html<String> {
    Html(format!(
        r#"<img src="{}">"#,
        assets.resolve(ui::img::LOGO_SVG)?,        // &'static str constant
    ))
}
```

The macro generates a module of `pub const` keys plus a `KEYS: &[&str]`
slice the manifest loader can sanity-check against on boot — surfacing
typos as `RK_E_MISSING_ASSET_KEY` rather than runtime 404s.

### 6.5 Hot-reload of manifest

In development, the manifest can be reloaded on file change:

```rust
let manifest = AssetManifest::watch("dist/assets-manifest.json").await?;
container.bind_singleton(manifest);
```

`AssetManifest::watch` returns the same `AssetManifest` type but with an
internal `RwLock` that swaps on notify events. `resolve()` always reads
the latest snapshot. Production builds use `AssetManifest::load` (no
watcher, immutable, no lock cost).

### 6.6 Static file serving

Asset *resolution* is separate from asset *serving*. For serving, use
`tower_http::services::ServeDir`:

```rust
bootstrap()
    .static_dir("/static", "dist/static")              // sugar over tower_http
    .modules(modules())
    // ...
```

`.static_dir(prefix, path)` mounts a `ServeDir` at the prefix. The
manifest contains the URL prefix (`/static/...`) so resolution and
serving line up.

---

## 7. CLI (`cargo rustkick`)

### 7.1 Distribution

Shipped as a **cargo subcommand**: install via
`cargo install rustkick-cli` and invoke as `cargo rustkick <subcommand>`.
This is the Rust convention (mirrors `cargo-edit`, `cargo-watch`,
`cargo-sqlx`) and avoids polluting `$PATH` with a bespoke binary name.

### 7.2 Command surface (v0.1)

```
cargo rustkick new <name>            # Scaffold a new app
cargo rustkick dev                   # cargo watch -x run, with extras
cargo rustkick g <kind> <name>       # Generate code
cargo rustkick add <package>         # Add a rustkick adapter/plugin to Cargo.toml
cargo rustkick info                  # Print framework + project versions
cargo rustkick check                 # Static check: DI graph, contributor cycles
```

### 7.3 Project scaffolding (`new`)

```bash
cargo rustkick new my-api
cargo rustkick new my-api --template rest    # rest | minimal | ddd
cargo rustkick new my-api --pg               # include rustkick-pg + migrations dir
cargo rustkick new my-api --yes              # accept all defaults non-interactively
```

Templates (mirroring KickJS `kick new`):

| Template  | Layout                                                                |
|-----------|-----------------------------------------------------------------------|
| `minimal` | `src/main.rs` + `src/modules/hello/mod.rs` only (~30 LOC)             |
| `rest`    | `src/modules/{users,health}/` with handlers + services (default)      |
| `ddd`     | `src/modules/{users}/{domain,application,infrastructure}/` layout     |

### 7.4 Code generators (`g`)

```bash
cargo rustkick g module users           # Full module: handlers, service, repository, model
cargo rustkick g module users --repo pg # …with a sqlx-backed Postgres repo
cargo rustkick g controller users       # Just a handlers.rs file with empty CRUD stubs
cargo rustkick g service payment        # Single #[service] struct
cargo rustkick g adapter websocket      # Adapter scaffold with every lifecycle hook stubbed
cargo rustkick g plugin analytics       # Plugin scaffold with register/layers/contributors
cargo rustkick g contributor tenant     # ContextContributor impl with typed Deps
cargo rustkick g migration add_users    # Wraps `sqlx migrate add`
```

Generators emit **fully-stubbed** files (every trait method present with
`todo!()` or sensible default + JSDoc-equivalent rustdoc) — same
philosophy as KickJS: you delete what you don't need rather than guess
what you have to add.

### 7.5 Dev loop (`dev`)

```bash
cargo rustkick dev
cargo rustkick dev --port 4000
cargo rustkick dev --no-watch          # one-shot run, no recompile loop
```

Under the hood: `cargo watch -x 'run --bin <app>'`, with:
- Colored startup banner (rustkick version, listen addr, mounted plugins/adapters)
- A `Ctrl+C` handler that triggers graceful shutdown via SIGTERM
- Optional asset manifest watcher if `--with-assets dist/manifest.json` is passed

If `cargo-watch` isn't installed, the CLI tells the user how to install it
and falls back to a single `cargo run`.

### 7.6 Package management (`add`)

```bash
cargo rustkick add pg                 # Adds rustkick-pg with version pinned to framework
cargo rustkick add auth swagger ws    # Multiple at once
cargo rustkick add --list             # Print catalog
```

Wraps `cargo add` with the right crate name (`rustkick-<x>`), version
matching, and any required feature flags. Refuses to install crates not
in the rustkick catalog (with a "did you mean…?" hint).

### 7.7 Static analysis (`check`)

```bash
cargo rustkick check
```

Parses `src/main.rs` (or a configured entry point), simulates the
`bootstrap()` builder calls via the same code path the runtime uses,
and reports:

- Missing DI tokens → `RK_E_UNKNOWN_TOKEN`
- Contributor dependency cycles → `RK_E_CONTRIBUTOR_CYCLE`
- Duplicate plugin/adapter names → `RK_E_DUPLICATE_MOUNT`
- Unused providers (warning)

Runs in CI before `cargo test` to surface graph errors faster than waiting
for the binary to start.

### 7.8 Config file: `rustkick.toml`

Optional, lives next to `Cargo.toml`. Drives CLI defaults — equivalent
to KickJS `kick.config.ts`.

```toml
[project]
pattern   = "ddd"                       # rest | minimal | ddd
entry     = "src/main.rs"

[modules]
dir       = "src/modules"
default_repo = "pg"                    # pg | inmemory | custom

[generate]
# pluralize module names by default (users vs user)
pluralize = true
```

### 7.9 Implementation crate

`rustkick-cli` is a normal binary crate (`bin/main.rs`) using `clap` v4
for arg parsing and `tera`/`minijinja` for template rendering. Templates
live in `crates/rustkick-cli/templates/` and are embedded with
`include_str!`.

---

## 8. Public API Surface

```rust
// rustkick prelude — what `use rustkick::*;` brings in
pub use rustkick_core::{
    Container, ContainerBuilder, Token, Scope,
    Module, define_module, define_modules, ModuleList,
    Adapter, define_adapter, AdapterContext,
    Plugin, define_plugin,
    ContextContributor, define_contributor,
    KickError, KickResult,
};

pub use rustkick_http::{
    bootstrap, Bootstrap,
    Ctx, RequestContext,
    Inject,                       // axum extractor
    routes::{Get, Post, Put, Patch, Delete},
};

#[cfg(feature = "macros")]
pub use rustkick_macros::{service, handler, get, post, put, patch, delete};

#[cfg(feature = "config")]
pub use rustkick_config::{ConfigService, define_env};
```

---

## 9. Sample: Users CRUD with Postgres

This is what the `examples/users-api` directory will contain.

### 6.1 `src/main.rs`

```rust
use rustkick::{bootstrap, KickResult};
use users_api::{adapters::pg_adapter, config::load_env, modules::modules};

#[tokio::main]
async fn main() -> KickResult<()> {
    let env = load_env()?;
    bootstrap()
        .adapter(pg_adapter(env.database_url.clone()))   // local to this example
        .modules(modules())
        .listen(&env.bind_addr)
        .await
}
```

> The DB adapter lives **inside this example crate** (`src/adapters/pg.rs`)
> rather than in the framework. Rustkick stays DB-agnostic; the example
> demonstrates the pattern any adopter would follow for their own ORM /
> driver of choice.

### 6.2 `src/modules/users/mod.rs`

```rust
pub mod handlers;
pub mod model;
pub mod repository;
pub mod service;

use rustkick::define_module;
pub use service::UserService;
pub use repository::UserRepository;

pub fn users_module() -> rustkick::Module {
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
```

### 6.3 `src/modules/users/service.rs`

```rust
use rustkick::Inject;
use uuid::Uuid;
use crate::modules::users::{model::User, repository::UserRepository};

#[rustkick::service]
pub struct UserService {
    repo: Inject<UserRepository>,
}

impl UserService {
    pub async fn list(&self) -> sqlx::Result<Vec<User>> {
        self.repo.find_all().await
    }
    pub async fn create(&self, email: &str, name: &str) -> sqlx::Result<User> {
        let user = User { id: Uuid::now_v7(), email: email.into(), name: name.into() };
        self.repo.insert(&user).await?;
        Ok(user)
    }
    // … show / update / delete elided for brevity
}
```

### 6.4 `src/modules/users/handlers.rs`

```rust
use axum::Json;
use rustkick::{Ctx, Inject, KickResult};
use serde::Deserialize;
use uuid::Uuid;

use crate::modules::users::{model::User, service::UserService};

pub async fn list(svc: Inject<UserService>) -> KickResult<Json<Vec<User>>> {
    Ok(Json(svc.list().await?))
}

#[derive(Deserialize)]
pub struct CreateUserBody {
    email: String,
    name: String,
}

pub async fn create(
    svc: Inject<UserService>,
    Json(body): Json<CreateUserBody>,
) -> KickResult<Json<User>> {
    Ok(Json(svc.create(&body.email, &body.name).await?))
}

// show / update / delete elided
```

### 6.5 `src/config.rs`

```rust
use rustkick_config::define_env;

define_env! {
    pub struct Env {
        pub database_url: String,
        pub bind_addr: String = "0.0.0.0:3000".into(),
        pub log_level: String = "info".into(),
    }
}

pub fn load_env() -> rustkick::KickResult<Env> {
    Env::from_env()
}
```

### 6.6 Cargo features the example uses

```toml
[dependencies]
rustkick = { path = "../../crates/rustkick" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
sqlx = { version = "0.8", features = ["postgres", "uuid", "runtime-tokio"] }
uuid = { version = "1", features = ["v7", "serde"] }
```

### 6.7 Running

```bash
# from examples/users-api/
sqlx database create
sqlx migrate run
cargo run
```

---

## 10. Comparison: KickJS → rustkick

| KickJS                            | rustkick                                   | Notes |
|-----------------------------------|--------------------------------------------|-------|
| `@Controller('/users')`           | `define_module().prefix("/users")`         | Module = controller + module rolled in one. |
| `@Get('/:id') show(ctx) { … }`    | `.get("/:id", handlers::show)`             | Or `#[get("/:id")]` with the macros feature. |
| `@Service() class UserService`    | `#[rustkick::service] struct UserService`  | Or just `.service::<UserService>()` registration. |
| `@Autowired private repo: Repo`   | `repo: Inject<Repo>` field                 | Constructor-style; `#[service]` generates the wiring. |
| `@Inject('app/users/repo')`       | `Inject<USER_REPO>` where `USER_REPO: Token<…>` | Tokens carry names into errors. |
| `defineHttpContextDecorator`      | `impl ContextContributor for X`            | Typed `Deps` make missing-dep an *unused import*, not a runtime error. |
| `defineAdapter`                   | `define_adapter()`                         | Same shape, async hooks. |
| `definePlugin`                    | `define_plugin()`                          | Same. |
| `bootstrap({ modules, adapters })`| `bootstrap().modules(…).adapter(…).await`  | Builder, not options struct. |
| `.scoped(name, cfg)` factory      | `factory.scoped(name, cfg)`                | Identical surface, namespaced DI tokens. |
| `.async({ inject, useFactory })`  | `factory.async_(opts)`                     | Deferred config resolution; resolved in `before_start`. |
| `MutableModuleRegistry`           | `define_modules().setup(\|reg\| …)`        | Conditional/dynamic mount. |
| `introspect()` on adapter/plugin  | `impl Introspect for X`                    | Same snapshot shape, feeds `/__debug`. |
| `Container.onChange(listener)`    | `container.on_change(closure)`             | Same 50ms debounce, same events. |
| `MiddlewareEntry { path, handler }`| `MiddlewareEntry::at(path, layer)`        | Path-scoped tower layer. |
| `ctx.paginate(handler, config)`   | `ctx.paginate(items, total)`               | Same wire format (items + page meta). |
| `Zod` validation                  | `serde` + `validator` + `schemars`         | Auto-derived OpenAPI via `utoipa`/`aide`. |
| `kick typegen`                    | (not needed — Rust has real types)         | Routes/params/env keys typed natively. |
| `kick dev` (Vite HMR)             | `cargo watch -x run`                       | Recompile loop, no in-process swap. |
| `kick g module users`             | `cargo rustkick gen module users`          | (deferred — v0.2.) |
| DevTools `/_debug`                | `rustkick-devtools` (deferred)             | Feature-gated JSON endpoint. |

---

## 11. Implementation Phases

### Phase 0 — Specs (this doc)
- [x] `SPEC.md`
- [ ] `ARCHITECTURE.md` (internals)

### Phase 1 — Core skeleton ✅ Done
- [x] `rustkick-core`: `Container` with three-scope resolution, `Module` provider folding,
      `define_adapter` / `define_plugin` factories (`.call()` / `.with()` / `.scoped()`),
      `KickError` + structured error matrix, generic Kahn `mount_sort`, `Introspect` contract
- [x] `rustkick-http`: `bootstrap().listen()` with real axum integration, `Inject<T>`
      `FromRequestParts` extractor, `HttpModule` with `.get`/`.post`/`.put`/`.patch`/`.delete`,
      `HttpError` (RFC 7807 problem-details), cooperative shutdown with per-adapter timeouts
- [x] `rustkick` umbrella crate re-exports HTTP `define_module` as the default

      45/45 tests passing (12 container · 9 module · 4 mount_sort · 6 adapter · 5 plugin · 9 http)

### Phase 2 — Sample app ✅ Done
- [x] `examples/users-api` runs end-to-end against a real Postgres
       (sqlx wiring lives inside the example, not in framework crates).
       Reversible sqlx-cli migrations applied in-process at startup
       via `sqlx::migrate!()`.
- [ ] `rustkick-config` (env loader) — deferred; example does its own
       trivial env load to stay self-contained.

### Phase 3 — Macros sugar
- [ ] `#[service]`, `#[handler]`, `#[get("/")]` / `#[post]` / etc.
- [ ] `define_env!` declarative macro

### Phase 4 — Context contributors
- [ ] `ContextContributor` trait with typed `Deps`
- [ ] Topo-sort + compile-time graph check via tuple-deps
- [ ] Integration with `Ctx::get<T>()`

### Phase 5 — Polish & extras
- [ ] Adapter shutdown with `tokio::join!` + timeout
- [ ] Structured request logging adapter
- [ ] OpenAPI generation (utoipa integration)
- [ ] Auth adapter (JWT)

### Phase 6 — Ecosystem (separate repos / crates, not in the foundation)
- [ ] `rustkick-ws` (WebSocket via axum/tungstenite)
- [ ] `rustkick-queue` (BullMQ-style, redis)
- [ ] `rustkick-otel` (tracing + metrics)
- [ ] `rustkick-devtools` (`/__debug` JSON endpoint)

> DB-related crates (`rustkick-pg`, `rustkick-diesel`, etc.) are intentionally
> excluded from the foundation roadmap. They can ship later as
> third-party crates if a real need surfaces.

---

## 12. Open Questions

1. **Generic-vs-trait-object DI.** `Inject<T>` returning `Arc<T>` works for
   concrete types, but trait-object services (`Inject<dyn Trait>`) need
   `Arc<dyn Trait>`. Decision: lean on `Token<dyn Trait>` for trait-object
   bindings, keep `Inject<T>` for concrete. Revisit if ergonomics suffer.

2. **Contributor dep encoding.** Tuple-typed `Deps` (`type Deps = (Tenant, User);`)
   is clean but limited to N tuples. Use a procedural macro
   `#[contributor(deps = [Tenant, User])]` to lift the limit cleanly.

3. **State vs Container.** Should `axum::State<AppState>` be wrapped around
   our container, or is `Inject<T>` the *only* path? Lean toward: ship both,
   `Inject<T>` is documented as canonical, `State` continues to work for
   axum-native code.

4. **Database integration.** v0.1 explicitly excludes DB code from
   framework crates. Adopters wire their ORM/driver as a normal adapter.
   A `rustkick-db` trait abstraction may land if a clear pattern
   emerges from real-world usage — but speculating now would lock
   premature design.

5. **Macro hygiene.** `#[service]` will need to generate a `From<&Container>`
   impl. Need to confirm this works under `paste`-free conditions; otherwise
   pull in `darling` + `syn`.

6. **Workspace publishing strategy.** Independent versions per crate (KickJS
   model) or lockstep? Lean independent — `cargo release` per crate, like
   `tokio-*` and `tower-*`.

---

**Next:** see `ARCHITECTURE.md` for internals — Container layout,
request lifecycle diagram, contributor topo-sort algorithm, error matrix.
