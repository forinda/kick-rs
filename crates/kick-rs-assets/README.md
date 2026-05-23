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

```rust,no_run
use kick_rs_assets::AssetManifest;

let m = AssetManifest::load("dist/.vite/manifest.json")?
    .with_url_prefix("/static");

let url = m.resolve("app.js")?;
// "/static/app.a1b2c3.js"
# Ok::<_, kick_rs_core::KickError>(())
```

Two JSON shapes are accepted out of the box.

**Flat** — the lowest-common-denominator format:

```json
{
  "app.js":  "app.a1b2c3.js",
  "app.css": "app.d4e5f6.css"
}
```

**Vite's full manifest** — what `vite build` emits when
`build.manifest = true`. Use `AssetManifest::from_vite_json(...)`:

```json
{
  "src/main.js": {
    "file": "assets/main.4889e940.js",
    "src": "src/main.js",
    "isEntry": true,
    "imports": ["_shared.83069a53.js"],
    "css":     ["assets/main.b82dbe22.css"]
  }
}
```

We reduce each entry to its `file` field; `resolve("src/main.js")`
returns the hashed JS URL. CSS / imports / dynamic assets aren't
surfaced separately yet — coming in a follow-up.

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

Disable with `default-features = false` if you only need the
manifest loader without the proc-macro compile cost.

The `embed_assets!` macro emits paths through `kick_rs_assets` itself
(via `proc-macro-crate` resolution), so adopters need *only*
`kick-rs-assets` in their `Cargo.toml` — no `include_dir` or any
other vendored dep.

## Install

```toml
[dependencies]
kick-rs-assets = "0.1.0-alpha.1"

# or via the umbrella crate with the `assets` feature:
kick-rs = { version = "0.1.0-alpha.3", features = ["assets"] }
```

## License

MIT — see the workspace root.
