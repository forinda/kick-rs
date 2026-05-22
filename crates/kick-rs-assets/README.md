# kick-rs-assets

> Typed asset manifest + compile-time embedding for
> [kick-rs](https://github.com/forinda/kick-rs).

[![crates.io](https://img.shields.io/crates/v/kick-rs-assets.svg)](https://crates.io/crates/kick-rs-assets)
[![docs.rs](https://docs.rs/kick-rs-assets/badge.svg)](https://docs.rs/kick-rs-assets)
[![license](https://img.shields.io/crates/l/kick-rs-assets.svg)](https://github.com/forinda/kick-rs/blob/main/LICENSE)

Two primitives for shipping static assets with a `kick-rs` app:

1. **`AssetManifest`** — load a flat `{key → hashed-filename}` JSON
   manifest (the kind webpack / vite / esbuild emit), then resolve
   logical keys to cache-busted URLs.
2. **`embed_assets!`** — a re-export of the
   [`include_dir!`](https://docs.rs/include_dir) macro that bundles a
   directory tree into the binary at compile time. Gated on the
   default `embed` feature.

HTTP serving (responding to `GET /static/...` with the right
content-type, cache headers, etc.) lives in **`kick-rs-http`**'s
`AssetsPlugin` so this crate stays free of axum.

## Manifest

```rust
use kick_rs_assets::AssetManifest;

let m = AssetManifest::load("dist/.vite/manifest.json")?
    .with_url_prefix("/static");

let url = m.resolve("app.js")?;
// "/static/app.a1b2c3.js"
```

Accepted JSON shape:

```json
{
  "app.js":  "app.a1b2c3.js",
  "app.css": "app.d4e5f6.css"
}
```

Errors are `KickError`-typed: `RK_C_IO` (read failure), `RK_C_PARSE`
(bad JSON), `RK_C_UNKNOWN_ASSET` (key not in manifest, hint includes
the catalog).

## Embedded assets (default-on)

```rust,ignore
use kick_rs_assets::{embed_assets, EmbeddedAssets, content_type_for};

static ASSETS: EmbeddedAssets = embed_assets!("$CARGO_MANIFEST_DIR/dist");

if let Some(file) = ASSETS.get_file("app.a1b2c3.js") {
    let mime = content_type_for("app.a1b2c3.js");
    // application/javascript; charset=utf-8
    serve(mime, file.contents());
}
```

Disable with `default-features = false` if you only want the manifest
loader without the `include_dir` build cost.

**Adopter requirement:** the `embed_assets!` macro expands to
`::include_dir::*` paths, so the consuming crate must list
`include_dir` in its own `Cargo.toml`:

```toml
[dependencies]
kick-rs-assets = "0.1.0-alpha.1"
include_dir    = "0.7"  # ← required for embed_assets!()
```

(We can't shield you from that without writing a proc-macro wrapper
that emits paths through `kick_rs_assets::__private` — planned.)

## Install

```toml
[dependencies]
kick-rs-assets = "0.1.0-alpha.1"

# or via the umbrella crate with the `assets` feature:
kick-rs = { version = "0.1.0-alpha.3", features = ["assets"] }
```

## License

MIT — see the workspace root.
