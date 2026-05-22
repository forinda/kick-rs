//! # kick-rs-assets
//!
//! Two small primitives for shipping static assets with a `kick-rs`
//! app:
//!
//! 1. [`AssetManifest`] — a flat `{key → hashed-filename}` map loaded
//!    from a webpack / vite / esbuild-style JSON manifest. Resolves
//!    a logical key (`"app.js"`) to the cache-busted URL the browser
//!    should fetch (`"/static/app.a1b2c3.js"`).
//! 2. `embed_assets!` — a re-export of [`include_dir!`] that bundles
//!    a directory tree into the binary at compile time, returning an
//!    [`EmbeddedAssets`] handle for lookup at runtime. Gated on the
//!    default `embed` feature.
//!
//!    **Note**: the macro expands to `::include_dir::*` paths, so
//!    consumers must add `include_dir = "0.7"` to their own
//!    `Cargo.toml`. We can't shield adopters from that without a
//!    proc-macro wrapper (planned).
//!
//! HTTP serving (responding to `GET /static/...` with the right
//! content-type, cache headers, and falling through to the manifest)
//! lives in [`kick-rs-http`]'s `AssetsPlugin` — kept there so this
//! crate stays free of axum.
//!
//! ```no_run
//! use kick_rs_assets::AssetManifest;
//!
//! let m = AssetManifest::load("dist/.vite/manifest.json")?
//!     .with_url_prefix("/static");
//!
//! let url = m.resolve("app.js")?; // "/static/app.a1b2c3.js"
//! # Ok::<_, kick_rs_core::KickError>(())
//! ```
//!
//! [`include_dir!`]: https://docs.rs/include_dir/latest/include_dir/macro.include_dir.html
//! [`kick-rs-http`]: https://docs.rs/kick-rs-http

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

use kick_rs_core::{KickError, KickResult};
use std::collections::BTreeMap;
use std::path::Path;

/// Map of logical asset keys (`"app.js"`) to their hashed filenames
/// (`"app.a1b2c3.js"`), plus an optional URL prefix prepended at
/// resolve time.
///
/// The JSON shape we accept is the lowest common denominator —
/// a flat object of `key: string` entries:
///
/// ```json
/// {
///   "app.js":  "app.a1b2c3.js",
///   "app.css": "app.d4e5f6.css"
/// }
/// ```
///
/// (Vite's full manifest shape — with nested `imports`, `css`, etc. —
/// can be reduced to this with the `--manifest=flat` build flag or a
/// small post-build script. A future revision of this crate may
/// accept the full shape directly.)
#[derive(Debug, Default, Clone)]
pub struct AssetManifest {
    entries: BTreeMap<String, String>,
    url_prefix: String,
}

impl AssetManifest {
    /// Read + parse a manifest from disk. Errors fall into two codes:
    /// `RK_C_IO` for read failure, `RK_C_PARSE` for malformed JSON.
    pub fn load<P: AsRef<Path>>(path: P) -> KickResult<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(|e| {
            KickError::new(
                "RK_C_IO",
                format!("could not read asset manifest `{}`: {e}", path.display()),
            )
        })?;
        Self::from_json(&raw).map_err(|e| {
            // Re-wrap to mention the file path, since the from_json
            // error doesn't know where the string came from.
            KickError::new(e.code, format!("{} (file: {})", e.message, path.display()))
        })
    }

    /// Parse a manifest from a JSON string. Useful for tests + when
    /// the manifest is bundled into the binary via `embed_assets!`.
    pub fn from_json(json: &str) -> KickResult<Self> {
        let entries: BTreeMap<String, String> = serde_json::from_str(json)
            .map_err(|e| KickError::new("RK_C_PARSE", format!("invalid asset manifest: {e}")))?;
        Ok(Self {
            entries,
            url_prefix: String::new(),
        })
    }

    /// Set the URL prefix prepended to every resolved value. Trailing
    /// slashes are normalized away so adopters get the expected
    /// joined form regardless of input.
    pub fn with_url_prefix(mut self, prefix: impl Into<String>) -> Self {
        let mut p = prefix.into();
        while p.ends_with('/') {
            p.pop();
        }
        self.url_prefix = p;
        self
    }

    /// The current URL prefix (without trailing slash).
    pub fn url_prefix(&self) -> &str {
        &self.url_prefix
    }

    /// Look up the versioned URL for `key`. Returns
    /// `<url_prefix>/<hashed_filename>`. Errors with
    /// `RK_C_UNKNOWN_ASSET` if the key isn't in the manifest, with a
    /// list of known keys in the hint.
    pub fn resolve(&self, key: &str) -> KickResult<String> {
        let hashed = self.entries.get(key).ok_or_else(|| {
            let known: Vec<&str> = self.entries.keys().map(String::as_str).collect();
            KickError::new(
                "RK_C_UNKNOWN_ASSET",
                format!("no asset entry for key `{key}`"),
            )
            .with_hint(format!(
                "known keys: {}",
                if known.is_empty() {
                    "<none — manifest is empty>".into()
                } else {
                    known.join(", ")
                }
            ))
        })?;
        if self.url_prefix.is_empty() {
            Ok(format!("/{hashed}"))
        } else {
            Ok(format!("{}/{}", self.url_prefix, hashed))
        }
    }

    /// Iterate `(key, hashed_filename)` pairs in key order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Number of entries in the manifest.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the manifest has any entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ─────────────────────────── Embedded assets ─────────────────────────────
#[cfg(feature = "embed")]
pub use embed::*;

#[cfg(feature = "embed")]
mod embed {
    //! Compile-time bundling via [`include_dir`].

    use kick_rs_core::{KickError, KickResult};

    /// Re-export of [`include_dir!`] under our own name so adopters
    /// can write `kick_rs_assets::embed_assets!(...)` without an
    /// extra dependency line.
    ///
    /// ```ignore
    /// use kick_rs_assets::{embed_assets, EmbeddedAssets};
    ///
    /// static ASSETS: EmbeddedAssets = embed_assets!("$CARGO_MANIFEST_DIR/dist");
    ///
    /// fn handler() {
    ///     if let Some(file) = ASSETS.get_file("app.a1b2c3.js") {
    ///         // serve file.contents()
    ///     }
    /// }
    /// ```
    pub use include_dir::include_dir as embed_assets;

    /// Bundled directory tree, addressable by path.
    pub type EmbeddedAssets = include_dir::Dir<'static>;

    /// Best-effort content-type guess from a file extension. Returns
    /// `application/octet-stream` for unknown extensions so the
    /// caller always has *something* safe to send.
    pub fn content_type_for(name: &str) -> &'static str {
        let lower = name.to_ascii_lowercase();
        let Some(dot) = lower.rfind('.') else {
            return "application/octet-stream";
        };
        match &lower[dot + 1..] {
            "html" | "htm" => "text/html; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "js" | "mjs" => "application/javascript; charset=utf-8",
            "json" => "application/json",
            "wasm" => "application/wasm",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "ico" => "image/x-icon",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "ttf" => "font/ttf",
            "otf" => "font/otf",
            "txt" | "text" => "text/plain; charset=utf-8",
            "map" => "application/json",
            _ => "application/octet-stream",
        }
    }

    /// Read a file from the embedded tree as a `&[u8]`. Errors with
    /// `RK_C_UNKNOWN_ASSET` if the path isn't bundled.
    pub fn read_embedded<'a>(dir: &'a EmbeddedAssets, rel: &str) -> KickResult<&'a [u8]> {
        dir.get_file(rel)
            .map(include_dir::File::contents)
            .ok_or_else(|| {
                KickError::new(
                    "RK_C_UNKNOWN_ASSET",
                    format!("no embedded asset at `{rel}`"),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_json_parses_flat_object() {
        let m = AssetManifest::from_json(
            r#"{ "app.js": "app.a1b2c3.js", "app.css": "app.d4e5f6.css" }"#,
        )
        .unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m.entries().count(), 2);
        // BTreeMap order — keys are sorted, .css comes before .js.
        let pairs: Vec<_> = m.entries().collect();
        assert_eq!(pairs[0].0, "app.css");
        assert_eq!(pairs[1].0, "app.js");
    }

    #[test]
    fn from_json_rejects_malformed_input() {
        let err = AssetManifest::from_json("not json").unwrap_err();
        assert_eq!(err.code, "RK_C_PARSE");
    }

    #[test]
    fn resolve_prepends_prefix_with_normalized_slash() {
        let m = AssetManifest::from_json(r#"{ "app.js": "app.a1b2c3.js" }"#)
            .unwrap()
            .with_url_prefix("/static///");
        assert_eq!(m.url_prefix(), "/static");
        assert_eq!(m.resolve("app.js").unwrap(), "/static/app.a1b2c3.js");
    }

    #[test]
    fn resolve_without_prefix_starts_with_slash() {
        let m = AssetManifest::from_json(r#"{ "app.js": "app.a1b2c3.js" }"#).unwrap();
        assert_eq!(m.resolve("app.js").unwrap(), "/app.a1b2c3.js");
    }

    #[test]
    fn resolve_unknown_key_errors_with_catalog_in_hint() {
        let m = AssetManifest::from_json(r#"{ "a.js": "a.x.js", "b.js": "b.y.js" }"#).unwrap();
        let err = m.resolve("c.js").unwrap_err();
        assert_eq!(err.code, "RK_C_UNKNOWN_ASSET");
        let hint = err.fix_hint.as_deref().unwrap_or("");
        assert!(hint.contains("a.js"), "hint: {hint}");
        assert!(hint.contains("b.js"), "hint: {hint}");
    }

    #[test]
    fn load_reads_from_tempfile() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), r#"{ "app.js": "app.fff.js" }"#).unwrap();
        let m = AssetManifest::load(tmp.path()).unwrap();
        assert_eq!(m.resolve("app.js").unwrap(), "/app.fff.js");
    }

    #[test]
    fn load_missing_file_errors() {
        let err = AssetManifest::load("does-not-exist.json").unwrap_err();
        assert_eq!(err.code, "RK_C_IO");
    }

    #[cfg(feature = "embed")]
    #[test]
    fn content_type_for_common_extensions() {
        assert_eq!(
            content_type_for("app.js"),
            "application/javascript; charset=utf-8"
        );
        assert_eq!(content_type_for("app.css"), "text/css; charset=utf-8");
        // Case-insensitive — `HTML` works the same as `html`.
        assert_eq!(content_type_for("index.HTML"), "text/html; charset=utf-8");
        assert_eq!(content_type_for("logo.svg"), "image/svg+xml");
        assert_eq!(content_type_for("font.woff2"), "font/woff2");
        assert_eq!(content_type_for("noext"), "application/octet-stream");
        assert_eq!(content_type_for("weird.exotic"), "application/octet-stream");
    }
}
