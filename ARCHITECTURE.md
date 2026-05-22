# kick-rs — Architecture & Internals

> Companion to [`SPEC.md`](./SPEC.md). Where SPEC describes the public
> surface, this document describes how the pieces actually fit together
> at runtime.

---

## Table of Contents

1. [Container Internals](#1-container-internals)
2. [Request Lifecycle](#2-request-lifecycle)
3. [Module Composition](#3-module-composition)
4. [Adapter Lifecycle & Topo-Sort](#4-adapter-lifecycle--topo-sort)
5. [Context Contributor Pipeline](#5-context-contributor-pipeline)
6. [Error Model](#6-error-model)
7. [Cooperative Shutdown](#7-cooperative-shutdown)
8. [Macro Expansion Strategy](#8-macro-expansion-strategy)
9. [Threading & Send/Sync](#9-threading--sendsync)

---

## 1. Container Internals

### 1.1 Storage

```rust
pub struct Container {
    singletons: RwLock<HashMap<ProviderKey, Arc<dyn Any + Send + Sync>>>,
    factories:  RwLock<HashMap<ProviderKey, Factory>>,
    request_scoped: RwLock<HashMap<ProviderKey, RequestFactory>>,
    metadata: HashMap<ProviderKey, ProviderMeta>,         // name, registered_at, etc.
}

enum ProviderKey {
    TypeId(std::any::TypeId),                            // anonymous
    Named { type_id: TypeId, name: &'static str },       // Token<T>
}

type Factory       = Arc<dyn Fn(&Container) -> Arc<dyn Any + Send + Sync> + Send + Sync>;
type RequestFactory = Arc<dyn Fn(&RequestContext) -> Arc<dyn Any + Send + Sync> + Send + Sync>;
```

### 1.2 Builder phase

`ContainerBuilder` accumulates **provider specs** (not instances) so we can
validate the dependency graph before any instantiation. At `.build()` time:

1. Collect all `ProviderSpec` entries.
2. Walk each spec's declared deps; verify each appears as another provider.
3. Topo-sort singleton providers; instantiate in order (so `UserService`
   can resolve `UserRepository` from the partial container at build time).
4. Store transient/request factories without invoking them.

Errors at this stage:
- `RK_E_UNKNOWN_TOKEN` — dep not registered
- `RK_E_CYCLE` — singleton cycle
- `RK_E_AMBIGUOUS_BIND` — same `ProviderKey` registered twice

### 1.3 Resolution path (hot)

```rust
impl Container {
    pub fn resolve<T: 'static + Send + Sync>(&self) -> Arc<T> {
        // 1. Try singleton (RwLock read, no allocation)
        if let Some(arc) = self.singletons.read().get(&key_of::<T>()) {
            return Arc::clone(arc).downcast::<T>().unwrap();
        }
        // 2. Try transient factory
        if let Some(f) = self.factories.read().get(&key_of::<T>()) {
            return f(self).downcast::<T>().unwrap();
        }
        // 3. UNREACHABLE if builder validated correctly
        panic!("Container resolve() called for unregistered T — this is a framework bug");
    }
}
```

`unwrap()` and `panic!` are intentional: if the builder validated, these
paths are unreachable. If they fire, it's a framework bug, not user error.

### 1.4 Request-scoped resolution

Request-scoped providers live on `http::Extensions` of the request, not in
the container. Lookup is:

```rust
impl<'r> RequestContext<'r> {
    pub fn inject<T: 'static + Send + Sync>(&self) -> Arc<T> {
        if let Some(arc) = self.extensions.get::<Arc<T>>() {
            return Arc::clone(arc);
        }
        self.container.resolve::<T>()                    // fall through to singleton/transient
    }
}
```

This means a request-scoped value **shadows** a singleton of the same type
for the lifetime of the request — symmetric to KickJS request store
semantics.

---

## 2. Request Lifecycle

```
Incoming HTTP request
        │
        ▼
┌──────────────────────────┐
│ axum::Router             │
│  ├ Adapter middleware    │ ← TraceLayer, CORS, request-id, etc.
│  ├ Module middleware     │ ← per-module Tower layers
│  └ Route handler         │
└──────────┬───────────────┘
           │
           ▼
┌──────────────────────────┐
│ Build RequestContext     │
│  - request_id            │
│  - container ref         │
│  - empty extensions      │
└──────────┬───────────────┘
           │
           ▼
┌──────────────────────────┐
│ Run contributor pipeline │   ← topo-sorted at boot, runs sequentially
│  for each contributor:   │
│   1. resolve deps from   │
│      ctx.extensions      │
│   2. call resolve()      │
│   3. insert result into  │
│      ctx.extensions      │
└──────────┬───────────────┘
           │
           ▼
┌──────────────────────────┐
│ Extract handler args     │
│  - Inject<T> reads from  │
│    container or ctx      │
│  - Json<T>, Path<T>, …   │
│    use axum extractors   │
└──────────┬───────────────┘
           │
           ▼
┌──────────────────────────┐
│ Run handler              │
└──────────┬───────────────┘
           │
           ▼
┌──────────────────────────┐
│ Serialize response       │
│  - axum IntoResponse     │
│  - KickError → ProblemJSON
└──────────┬───────────────┘
           │
           ▼
       Response
```

### 2.1 Inject extractor

```rust
pub struct Inject<T: 'static>(pub Arc<T>);

impl<T, S> FromRequestParts<S> for Inject<T>
where
    T: 'static + Send + Sync,
    S: Send + Sync,
    Arc<Container>: FromRef<S>,
{
    type Rejection = KickError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let container = Arc::<Container>::from_ref(state);
        // Check request extensions for request-scoped first
        if let Some(arc) = parts.extensions.get::<Arc<T>>() {
            return Ok(Inject(Arc::clone(arc)));
        }
        Ok(Inject(container.resolve::<T>()))
    }
}

impl<T> std::ops::Deref for Inject<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}
```

This is the **entire DI integration with axum** — 30 lines of code.
Everything else is bookkeeping in the container.

---

## 3. Module Composition

```rust
pub struct Module {
    name: String,
    prefix: String,
    providers: Vec<ProviderSpec>,
    routes: Vec<RouteSpec>,
    contributors: Vec<Box<dyn AnyContributor>>,
    middleware: Vec<Box<dyn tower::Layer<…>>>,
    sub_modules: Vec<Module>,
}

struct RouteSpec {
    method: Method,
    path: String,                                        // joined with module prefix at build time
    handler: Box<dyn ErasedHandler>,
}
```

`define_modules().mount(users_module()).mount(orders_module())` returns a
`ModuleList` whose `.into_router(container)` produces an `axum::Router`
by iterating routes, attaching extractors, and nesting per-module
sub-routers.

Sub-modules:

```rust
define_module("api")
    .prefix("/api/v1")
    .sub_module(users_module())
    .sub_module(orders_module())
```

---

## 4. Adapter Lifecycle & Topo-Sort

### 4.1 Lifecycle phases

```
   bootstrap()
       │
       ├──► gather providers from all modules + adapters
       │
       ├──► topo-sort adapters by `depends_on`
       │       (DuplicateMountName / MissingMountDep / MountCycle errors here)
       │
       ├──► run Adapter::before_mount in topo order      ┐
       │                                                 │ container is mutable
       ├──► Container::build()                            │ here
       │                                                 ┘
       ├──► run Adapter::before_start                    ┐
       │                                                 │ container is sealed
       ├──► build axum::Router                            │
       │                                                 │
       ├──► axum::serve(...)                              │
       │                                                 │
       ├──► run Adapter::after_start                      │
       │                                                 │
       │   ── server running ──                          │
       │                                                 │
       └──► on shutdown: tokio::join!(all .shutdown())   ┘
```

### 4.2 Topo-sort algorithm

Kahn's algorithm, identical to KickJS `mountSort`:

```rust
fn topo_sort(items: Vec<MountItem>) -> KickResult<Vec<MountItem>> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();

    for item in &items {
        in_degree.entry(item.name).or_insert(0);
        for dep in item.depends_on {
            graph.entry(dep).or_default().push(item.name);
            *in_degree.entry(item.name).or_insert(0) += 1;
            if !items.iter().any(|i| i.name == *dep) {
                return Err(KickError::missing_mount_dep(item.name, dep));
            }
        }
    }

    let mut ready: VecDeque<&str> = in_degree.iter()
        .filter(|(_, d)| **d == 0)
        .map(|(n, _)| *n)
        .collect();

    let mut order = Vec::new();
    while let Some(name) = ready.pop_front() {
        order.push(name);
        for dependent in graph.get(name).into_iter().flatten() {
            let d = in_degree.get_mut(dependent).unwrap();
            *d -= 1;
            if *d == 0 { ready.push_back(dependent); }
        }
    }

    if order.len() != items.len() {
        return Err(KickError::mount_cycle(/* find cycle for nice error */));
    }
    /* ... reorder items ... */
}
```

---

## 5. Context Contributor Pipeline

### 5.1 The compile-time guarantee

```rust
pub trait ContextContributor: Send + Sync + 'static {
    type Key: 'static + Send + Sync;
    type Deps: ContributorDeps;

    async fn resolve(
        &self,
        ctx: &RequestContext,
        deps: <Self::Deps as ContributorDeps>::Resolved<'_>,
    ) -> KickResult<Self::Key>;
}

pub trait ContributorDeps {
    type Resolved<'a>;
    fn extract<'a>(ctx: &'a RequestContext) -> KickResult<Self::Resolved<'a>>;
}

// Implementations for tuples (n=0..8):
impl ContributorDeps for () {
    type Resolved<'a> = ();
    fn extract<'a>(_: &'a RequestContext) -> KickResult<()> { Ok(()) }
}

impl<A: 'static> ContributorDeps for (A,) {
    type Resolved<'a> = (&'a A,);
    fn extract<'a>(ctx: &'a RequestContext) -> KickResult<(&'a A,)> {
        Ok((ctx.get::<A>(),))
    }
}

impl<A: 'static, B: 'static> ContributorDeps for (A, B) {
    type Resolved<'a> = (&'a A, &'a B);
    fn extract<'a>(ctx: &'a RequestContext) -> KickResult<(&'a A, &'a B)> {
        Ok((ctx.get::<A>(), ctx.get::<B>()))
    }
}
// ... up to 8-tuples
```

`ctx.get::<A>()` returns `&A` directly — and if `A` was never produced by
an upstream contributor, the topo-sort at boot time catches it as
`KickError::missing_contributor("A")`.

### 5.2 Building the pipeline

```rust
pub struct ContributorPipeline {
    contributors: Vec<Box<dyn ErasedContributor>>,        // topo-sorted
}

trait ErasedContributor: Send + Sync {
    fn produces(&self) -> TypeId;
    fn requires(&self) -> &[TypeId];
    async fn run(&self, ctx: &mut RequestContext) -> KickResult<()>;
}
```

`Module::build()` topo-sorts the `Vec<Box<dyn ErasedContributor>>` once,
caches the order, and the pipeline runs sequentially per-request.

### 5.3 Why not run in parallel?

KickJS runs contributors sequentially even though they could parallelize
when they share no deps. We follow the same choice for v0.1: sequential
is predictable, easier to debug, and matches sequential `await` order
for short stacks. Parallel execution is a v0.4 opt-in flag.

---

## 6. Error Model

### 6.1 KickError shape

```rust
pub struct KickError {
    pub code: &'static str,         // "RK_E_UNKNOWN_TOKEN"
    pub message: String,
    pub fix_hint: Option<String>,
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
    pub context: BTreeMap<String, String>,               // arbitrary key=value
    pub status: Option<http::StatusCode>,                // for response mapping
}
```

### 6.2 Error code prefixes

| Prefix      | Layer              | Examples |
|-------------|--------------------|----------|
| `RK_E_*`    | Framework errors   | `RK_E_UNKNOWN_TOKEN`, `RK_E_CYCLE`, `RK_E_MISSING_CONTRIBUTOR` |
| `RK_H_*`    | HTTP / axum        | `RK_H_BAD_BODY`, `RK_H_MISSING_HEADER` |
| `RK_C_*`    | Config / env       | `RK_C_MISSING_ENV`, `RK_C_INVALID_ENV` |
| `RK_A_*`    | Reserved for app-/adapter-defined error codes | (e.g. an example pg adapter might use `RK_A_PG_POOL_EXHAUSTED`) |

### 6.3 Boot-time error matrix

| Code                       | Phase  | Cause                                | Fix hint |
|----------------------------|--------|--------------------------------------|----------|
| `RK_E_UNKNOWN_TOKEN`       | Build  | DI dep not registered                | "Register `{T}` via `.singleton::<{T}>()` or `.bind({TOKEN}, …)`." |
| `RK_E_CYCLE`               | Build  | Singleton dep cycle                  | "Break cycle: {a} → {b} → {a}." |
| `RK_E_AMBIGUOUS_BIND`      | Build  | Two providers for same key           | "Use named `Token<{T}>` to disambiguate." |
| `RK_E_INVALID_SCOPE`       | Build  | Singleton depends on request-scoped  | "Make `{singleton}` request-scoped, or remove the `{request_dep}` dependency." |
| `RK_E_DUPLICATE_MOUNT`     | Sort   | Two adapters with same `name`        | "Rename one of {a} / {b}." |
| `RK_E_MISSING_MOUNT_DEP`   | Sort   | `depends_on` references unknown name | "Add the missing adapter or remove the dep." |
| `RK_E_MOUNT_CYCLE`         | Sort   | Adapter `depends_on` cycle           | "Break cycle: {a} → {b} → {a}." |
| `RK_E_MISSING_CONTRIBUTOR` | Sort   | Contributor `Deps` not registered    | "Register a `ContextContributor` producing `{T}`." |
| `RK_E_CONTRIBUTOR_CYCLE`   | Sort   | Contributor dep cycle                | "Break cycle: {a} → {b} → {a}." |

### 6.4 IntoResponse for KickError

```rust
impl IntoResponse for KickError {
    fn into_response(self) -> Response {
        let status = self.status.unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = ProblemDetails {
            type_: format!("https://errors.kick-rs.dev/{}", self.code),
            title: self.message,
            status: status.as_u16(),
            detail: self.fix_hint,
            extensions: self.context,
        };
        (status, Json(body)).into_response()
    }
}
```

RFC 7807 Problem Details JSON — same shape as KickJS `Problems`.

---

## 7. Cooperative Shutdown

```rust
async fn shutdown(adapters: Vec<Arc<dyn Adapter>>, timeout: Duration) {
    let futures: Vec<_> = adapters.iter()
        .map(|a| {
            let name = a.name().to_string();
            let a = Arc::clone(a);
            async move {
                match tokio::time::timeout(timeout, a.shutdown()).await {
                    Ok(Ok(())) => tracing::info!(adapter = %name, "shut down cleanly"),
                    Ok(Err(e)) => tracing::warn!(adapter = %name, error = %e, "shutdown failed"),
                    Err(_)     => tracing::warn!(adapter = %name, "shutdown timed out"),
                }
            }
        })
        .collect();

    // Wait for all, even if some fail — equivalent to Promise.allSettled
    futures::future::join_all(futures).await;
}
```

### 7.1 SIGTERM handling

By default, kick-rs installs its own SIGTERM/SIGINT handler. If an
observability SDK (OpenTelemetry, Sentry) needs to own signal handling,
pass `process_hooks = ProcessHooks::ErrorsOnly` to `bootstrap()` to skip
the framework's installation — same toggle as KickJS.

```rust
bootstrap()
    .process_hooks(ProcessHooks::ErrorsOnly)             // OTel owns SIGTERM
    .adapter(otel_adapter)
    // ...
```

---

## 8. Macro Expansion Strategy

### 8.1 `#[service]`

Input:
```rust
#[kick-rs::service]
pub struct UserService {
    repo: Inject<UserRepository>,
    pool: Inject<PgPool>,
}
```

Expansion:
```rust
pub struct UserService {
    repo: Arc<UserRepository>,                           // Inject<T> → Arc<T>
    pool: Arc<PgPool>,
}

impl UserService {
    pub fn from_container(c: &Container) -> Arc<Self> {
        Arc::new(Self {
            repo: c.resolve::<UserRepository>(),
            pool: c.resolve::<PgPool>(),
        })
    }
}

impl ::kick-rs::Buildable for UserService {
    fn register(builder: &mut ContainerBuilder) {
        builder.singleton_factory::<Self, _>(Self::from_container);
    }
}
```

`.service::<UserService>()` on the builder calls `UserService::register(&mut self)`.

### 8.2 `#[get("/users/:id")]`

Input:
```rust
#[get("/users/:id")]
pub async fn show(svc: Inject<UserService>, ctx: Ctx<ShowParams>) -> KickResult<Json<User>> { ... }

#[derive(Deserialize)]
pub struct ShowParams { id: Uuid }
```

The macro records the route in a `inventory::collect!`-style registry so
`define_module().scan(module_path!())` can pick them up. Optional sugar —
the bare `.get("/users/:id", show)` form always works.

### 8.3 `define_env!`

Declarative macro (no proc-macro crate needed):

```rust
define_env! {
    pub struct Env {
        pub database_url: String,
        pub bind_addr: String = "0.0.0.0:3000".into(),
    }
}
```

Expands to a struct + `from_env() -> KickResult<Env>` that reads
`DATABASE_URL`, `BIND_ADDR`, applying defaults and emitting
`RK_C_MISSING_ENV` for required keys.

---

## 9. Threading & Send/Sync

### 9.1 Requirements

- All DI-managed types: `Send + Sync + 'static` (held in `Arc`).
- All handler futures: `Send` (axum requires this).
- All adapter futures: `Send` (we await across threads on shutdown).
- `Container` itself: `Send + Sync` (wrapped in `Arc`, shared across worker threads).

### 9.2 Pitfalls

- **`Rc` / `RefCell`** in services will fail compilation — explicit.
- **`!Send` futures** (`MutexGuard` held across `.await`) will fail at
  the handler signature — caught by axum's existing trait bounds.
- **Singleton mutable state** must use `tokio::sync::Mutex` or atomics,
  not `std::sync::Mutex` held across `.await`.

### 9.3 Why `Arc<T>` instead of `&'static T`

We could leak singletons via `Box::leak` for `&'static T` and skip the
Arc clone on every resolve. We don't, because:
1. Test isolation requires being able to *drop* a container.
2. Hot-reload (later) requires the same.
3. `Arc::clone` is cheap — atomic refcount bump, no allocation.

---

## Appendix A — File-by-file outline

```
crates/kick-rs-core/src/
├── lib.rs                        # public exports
├── container/
│   ├── mod.rs                   # Container, ContainerBuilder
│   ├── provider.rs              # ProviderSpec, ProviderKey
│   └── scope.rs                 # Scope enum
├── token.rs                      # Token<T>
├── module.rs                     # Module, define_module()
├── adapter.rs                    # Adapter trait, define_adapter()
├── plugin.rs                     # Plugin trait, define_plugin()
├── contributor/
│   ├── mod.rs                   # ContextContributor, ContributorDeps
│   ├── pipeline.rs              # build + run
│   └── topo.rs                  # topo-sort
├── error.rs                      # KickError, KickResult
└── mount_sort.rs                 # generic Kahn topo-sort

crates/kick-rs-http/src/
├── lib.rs                        # public exports
├── bootstrap.rs                  # bootstrap() builder
├── context.rs                    # RequestContext, Ctx<P>
├── inject.rs                     # Inject<T> extractor
├── routing.rs                    # Module → axum::Router conversion
└── middleware/
    ├── request_id.rs
    ├── request_logger.rs
    └── error_handler.rs

crates/kick-rs-macros/src/
├── lib.rs                        # proc-macro entry points
├── service.rs                    # #[service]
├── handler.rs                    # #[handler] / #[get] / etc.
└── contributor.rs                # #[contributor]

crates/kick-rs-config/src/
├── lib.rs
├── env.rs                        # define_env! + load_env()
└── service.rs                    # ConfigService

crates/kick-rs-assets/src/
├── lib.rs                        # AssetManifest, asset_keys! macro, UnknownAssetError
├── manifest.rs                   # load / watch
└── resolve.rs                    # resolve(key) + typed key registry

crates/kick-rs-cli/                # binary crate
├── src/main.rs                    # `cargo kick-rs` entry point
├── src/commands/{new,dev,gen,add,info,check}.rs
└── templates/                     # embedded code templates for `g`

# DB-specific crates are intentionally NOT part of the foundation —
# the users-api example puts its sqlx adapter under examples/users-api/src/adapters/.
```

---

**End of architecture document.** See `SPEC.md` for the user-facing API.
