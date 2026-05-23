#![doc = include_str!("../README.md")]
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
    //! Compile-time bundling via the `kick-rs-assets-macros` proc-macro.
    //!
    //! The tree is a plain `&'static` cascade — no `Box`, `Vec`, or
    //! lazy allocation. Every file's contents come from `include_bytes!`
    //! emitted by the proc-macro.

    use kick_rs_core::{KickError, KickResult};

    /// Embed a directory tree into the binary at compile time.
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
    ///
    /// Accepted path forms: absolute, `$CARGO_MANIFEST_DIR/...`,
    /// `$OUT_DIR/...`, or relative (resolved against
    /// `$CARGO_MANIFEST_DIR`).
    pub use kick_rs_assets_macros::embed_assets;

    /// One bundled directory. Static — every field lives in `'static`
    /// memory; iteration is cheap and allocation-free.
    #[derive(Debug, Clone, Copy)]
    pub struct EmbeddedAssets {
        path: &'static str,
        entries: &'static [EmbeddedEntry],
    }

    /// One bundled file.
    #[derive(Debug, Clone, Copy)]
    pub struct EmbeddedFile {
        path: &'static str,
        contents: &'static [u8],
    }

    /// Entry in an embedded tree — either a file or a sub-directory.
    #[derive(Debug, Clone, Copy)]
    pub enum EmbeddedEntry {
        /// A file with its bytes loaded via `include_bytes!`.
        File(EmbeddedFile),
        /// A nested directory.
        Dir(EmbeddedAssets),
    }

    impl EmbeddedAssets {
        // Constructor exposed for use by the proc-macro's expansion.
        // `pub` + `#[doc(hidden)]` is the established Rust pattern for
        // "callable from generated code, not from humans".
        #[doc(hidden)]
        pub const fn __new(path: &'static str, entries: &'static [EmbeddedEntry]) -> Self {
            Self { path, entries }
        }

        /// Path the tree was rooted at, relative to its parent. Empty
        /// string for the top-level tree.
        pub fn path(&self) -> &'static str {
            self.path
        }

        /// Direct child entries (one level — does not recurse).
        pub fn entries(&self) -> &'static [EmbeddedEntry] {
            self.entries
        }

        /// Find a file by its forward-slash-separated path relative
        /// to *this* directory. Walks sub-directories as needed.
        pub fn get_file(&self, rel: &str) -> Option<&'static EmbeddedFile> {
            // Strip a leading slash so `/foo.js` and `foo.js` both work.
            let rel = rel.strip_prefix('/').unwrap_or(rel);
            for entry in self.entries {
                match entry {
                    EmbeddedEntry::File(f) => {
                        if path_matches(f.path, self.path, rel) {
                            return Some(f);
                        }
                    }
                    EmbeddedEntry::Dir(d) => {
                        if let Some(f) = d.get_file(rel) {
                            return Some(f);
                        }
                    }
                }
            }
            None
        }
    }

    impl EmbeddedFile {
        // Same convention as EmbeddedAssets::__new.
        #[doc(hidden)]
        pub const fn __new(path: &'static str, contents: &'static [u8]) -> Self {
            Self { path, contents }
        }

        /// The file's path relative to the embedded tree's root.
        pub fn path(&self) -> &'static str {
            self.path
        }

        /// The file's bytes.
        pub fn contents(&self) -> &'static [u8] {
            self.contents
        }
    }

    /// Path-comparison helper. `file_path` is the file's path relative
    /// to the *root* of the embedded tree. `dir_prefix` is the path of
    /// the directory we're searching from. `target` is the path the
    /// caller is looking up, relative to `dir_prefix`. Returns true
    /// when `file_path == join(dir_prefix, target)`.
    fn path_matches(file_path: &str, dir_prefix: &str, target: &str) -> bool {
        if dir_prefix.is_empty() {
            return file_path == target;
        }
        // file_path should be `<dir_prefix>/<target>`. Avoid an alloc
        // by comparing in pieces.
        file_path
            .strip_prefix(dir_prefix)
            .and_then(|rest| rest.strip_prefix('/'))
            == Some(target)
    }

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
    pub fn read_embedded(dir: &EmbeddedAssets, rel: &str) -> KickResult<&'static [u8]> {
        dir.get_file(rel)
            .map(EmbeddedFile::contents)
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
