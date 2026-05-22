# rustkick-core

> Core DI container, modules, adapters, plugins, and structured error
> model for the [rustkick](https://github.com/forinda/rustkick) framework.

[![crates.io](https://img.shields.io/crates/v/rustkick-core.svg)](https://crates.io/crates/rustkick-core)
[![docs.rs](https://docs.rs/rustkick-core/badge.svg)](https://docs.rs/rustkick-core)
[![license](https://img.shields.io/crates/l/rustkick-core.svg)](https://github.com/forinda/rustkick/blob/main/LICENSE)

This crate has **no dependency on axum, hyper, or any HTTP runtime**.
That's intentional: rustkick's DI graph and module composition are
useful for non-HTTP transports too (queue workers, CLIs, future
WebSocket-only apps), so the primitives that don't care about HTTP
live here.

## Most users want [`rustkick`](https://crates.io/crates/rustkick)

The umbrella `rustkick` crate re-exports everything in this crate plus
the HTTP integration ([`rustkick-http`](https://crates.io/crates/rustkick-http)).
Depending on `rustkick-core` directly only makes sense if you're:

- Building a non-HTTP application (worker, CLI) that still wants
  rustkick's DI + module composition.
- Authoring a transport-level integration (e.g., a WebSocket-only
  bootstrap) and want to share the container/module model with HTTP.

If you're writing a web app, use the umbrella.

## What's in this crate

| Module             | What it provides                                              |
|--------------------|---------------------------------------------------------------|
| `container`        | `Container` + `ContainerBuilder` — three-scope DI (singleton, transient, request) |
| `token`            | `Token<T>` for named bindings (trait objects, disambiguation) |
| `scope`            | The `Scope` enum                                              |
| `module`           | `define_module()` builder, `Module`, `ProviderSpec`           |
| `adapter`          | `Adapter` trait + `define_adapter()` factory (`.call/.with/.scoped`) |
| `plugin`           | `Plugin` trait + `define_plugin()` factory (same shape)       |
| `contributor`      | `ContextContributor` trait (typed dependency declarations)    |
| `mount_sort`       | Generic Kahn topological sort (cycle / missing / duplicate detection) |
| `introspect`       | `Introspect` trait — opt-in DevTools snapshots                |
| `error`            | `KickError` (RFC 7807-compatible) + `KickResult`              |

## Quick example

```rust
use rustkick_core::{define_module, Container};
use std::sync::Arc;

struct Greeter(String);

let module = define_module("hello")
    .service_value(Greeter("world".into()))
    .build();

let container = module
    .register_into(Container::builder())
    .build()
    .unwrap();

let g = container.resolve::<Greeter>();
assert_eq!(g.0, "world");
```

The container detects duplicate registrations at build time with
`RK_E_AMBIGUOUS_BIND`, and singleton-factory closures can resolve
their own dependencies from the in-progress container.

## Install

```toml
[dependencies]
rustkick-core = "0.0"
```

## Status

Early — at `0.0.x`. API surface is reserved but may shift before
`v0.1.0`. Pin to a specific version (not a range) for reproducible
builds during the pre-release window. See
[`RELEASE.md`](https://github.com/forinda/rustkick/blob/main/RELEASE.md)
for the versioning model.

## License

MIT — see the workspace root.
