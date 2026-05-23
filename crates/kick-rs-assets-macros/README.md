# kick-rs-assets-macros

> Proc-macro half of [kick-rs-assets](https://crates.io/crates/kick-rs-assets).
> Ships `embed_assets!`.

[![crates.io](https://img.shields.io/crates/v/kick-rs-assets-macros.svg)](https://crates.io/crates/kick-rs-assets-macros)
[![docs.rs](https://docs.rs/kick-rs-assets-macros/badge.svg)](https://docs.rs/kick-rs-assets-macros)
[![license](https://img.shields.io/crates/l/kick-rs-assets-macros.svg)](https://github.com/forinda/kick-rs/blob/main/LICENSE)

You don't depend on this crate directly. Add `kick-rs-assets` with
the (default) `embed` feature and it'll pull this in:

```toml
[dependencies]
kick-rs-assets = "0.1"
```

Then:

```rust,ignore
use kick_rs_assets::{embed_assets, EmbeddedAssets};

static ASSETS: EmbeddedAssets = embed_assets!("$CARGO_MANIFEST_DIR/dist");
```

The macro walks the target directory at compile time and emits a
static cascade of `kick-rs-assets`' runtime types with file bytes
pulled in via `include_bytes!`. Paths through `kick_rs_assets` are
resolved by [`proc-macro-crate`](https://crates.io/crates/proc-macro-crate),
so adopters never need to depend on `include_dir` or any other
vendored crate.

Accepted path forms:

- `$CARGO_MANIFEST_DIR/...`
- `$OUT_DIR/...`
- Absolute (`/path/to/dist` on Unix, `C:\path\to\dist` on Windows)
- Relative — resolved against `$CARGO_MANIFEST_DIR`

Directory entries are stable-sorted so the emitted tree is
deterministic across builds.

## License

MIT — see the workspace root.
