# kick-rs-macros

> Opt-in proc-macro sugar for the [kick-rs](https://github.com/forinda/kick-rs)
> framework — currently `#[service]`, with more landing as later phases
> complete.

[![crates.io](https://img.shields.io/crates/v/kick-rs-macros.svg)](https://crates.io/crates/kick-rs-macros)
[![docs.rs](https://docs.rs/kick-rs-macros/badge.svg)](https://docs.rs/kick-rs-macros)
[![license](https://img.shields.io/crates/l/kick-rs-macros.svg)](https://github.com/forinda/kick-rs/blob/main/LICENSE)

Every macro here expands to a call you could write yourself against
[`kick-rs-core`](https://crates.io/crates/kick-rs-core). The macros are
sugar over the existing builder API — never required.

## Most users want [`kick-rs`](https://crates.io/crates/kick-rs) with `features = ["macros"]`

```toml
[dependencies]
kick-rs = { version = "0.0", features = ["macros"] }
```

That re-exports `#[service]` directly off the umbrella so you write
`use kick_rs::service;`. Depending on this crate directly is for
specialized cases — most apps don't need to.

## `#[service]`

Derives [`ServiceImpl`](https://docs.rs/kick-rs-core/latest/kick_rs_core/trait.ServiceImpl.html)
from a struct definition, letting modules register it with one call:

```rust
use kick_rs::{define_module, service, Inject};
use std::sync::Arc;

struct PgPool { /* ... */ }
struct UserRepository { /* ... */ }

# kick_rs::define_module("noop")
#     .service_value(PgPool {})
#     .service_value(UserRepository {})

#[service]
pub struct UserService {
    repo: Inject<UserRepository>,   // rewritten to Arc<UserRepository>
    pool: Arc<PgPool>,              // kept as-is
}
```

Field rules:

| Type        | Behavior in the rewritten struct | In generated `build()`        |
|-------------|----------------------------------|-------------------------------|
| `Inject<T>` | rewritten to `Arc<T>`            | `c.resolve::<T>()`            |
| `Arc<T>`    | left as-is                       | `c.resolve::<T>()`            |
| other       | compile error                    | (use `.service_value(value)`) |

Unit structs work too (expand to `Arc::new(Foo)`). Tuple structs are
rejected — name the fields or write the `ServiceImpl` impl by hand.

The macro resolves the trait path at expansion time via
[`proc-macro-crate`](https://crates.io/crates/proc-macro-crate), so it
works whether you import via the umbrella `kick-rs` or
[`kick-rs-core`](https://crates.io/crates/kick-rs-core) directly.

## `#[contributor]`

Function-style sugar that turns an `async fn` into a
[`ContextContributor`](https://docs.rs/kick-rs-core/latest/kick_rs_core/trait.ContextContributor.html)
unit struct. Parallel in spirit to `#[service]`, but for the request-
context pipeline.

```rust
use kick_rs::{contributor, ContributorRequest, ContributorRequestExt, KickResult};

#[contributor]
async fn LoadTenant() -> KickResult<Tenant> {
    Ok(Tenant { id: 42 })
}

#[contributor]
async fn LoadProject(tenant: &Tenant) -> KickResult<Project> {
    Ok(Project { tenant_id: tenant.id })
}

#[contributor]
async fn LoadTenantDb(
    ctx: &dyn ContributorRequest,    // optional — gives access to DI
    tenant: &Tenant,
) -> KickResult<TenantDb> {
    let cfg = ctx.inject::<TenantConfig>();
    Ok(TenantDb::for_tenant(&tenant.slug, cfg.pool_size).await?)
}
```

Rules:

| Function shape                                       | Generated `Deps` |
|------------------------------------------------------|------------------|
| `async fn X() -> KickResult<T>`                      | `()`             |
| `async fn X(a: &A) -> KickResult<T>`                 | `(A,)`           |
| `async fn X(a: &A, b: &B) -> KickResult<T>`          | `(A, B)`         |
| `async fn X(ctx: &dyn ContributorRequest, a: &A, …)` | `(A, …)` — ctx threaded in |

- Must be `async fn`.
- Return type must be `KickResult<KeyType>` — the inner type becomes `Key`.
- First parameter may be named `ctx` (or `_ctx`) with type
  `&dyn ContributorRequest` to access DI inside the body.
- Remaining `&T` parameters become `Deps` in declaration order.
- Stateful contributors (those holding fields) still need a manual
  `impl ContextContributor` — this macro only covers unit-struct.

Use PascalCase function names — they become the generated struct.

## `#[handler]`

Currently a pass-through placeholder. Reserved for future codegen
integration (e.g., emitting a route registry entry the
[`cargo kick`](https://github.com/forinda/kick-rs) CLI can read).

## Install (standalone)

```toml
[dependencies]
kick-rs-core    = "0.0"
kick-rs-macros  = "0.0"
```

You'll want `kick-rs-core` in scope because the macro emits references
to its types.

## Status

Early — at `0.0.x`. API surface is reserved but may shift before
`v0.1.0`. See
[`RELEASE.md`](https://github.com/forinda/kick-rs/blob/main/RELEASE.md)
for the versioning model.

## License

MIT — see the workspace root.
