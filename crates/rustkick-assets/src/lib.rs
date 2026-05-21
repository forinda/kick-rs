//! # rustkick-assets
//!
//! Typed asset manifest with cache-busting URL resolution.
//! See [`SPEC.md` §6](../SPEC.md#6-assets) for usage.
//!
//! Concrete implementation lands in Phase 5 — this file reserves the
//! public surface so dependants can import the types today.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

use rustkick_core::{KickError, KickResult};
use std::collections::HashMap;
use std::path::Path;

/// Loaded asset manifest. Maps source keys (`"app.js"`) to versioned
/// URLs (`"/static/app.a1b2c3.js"`).
#[derive(Debug, Default, Clone)]
pub struct AssetManifest {
    entries: HashMap<String, String>,
    version: u32,
}

impl AssetManifest {
    /// Read a manifest from disk. Immutable for the process lifetime.
    pub fn load<P: AsRef<Path>>(_path: P) -> KickResult<Self> {
        Err(KickError::new(
            "RK_E_UNIMPLEMENTED",
            "AssetManifest::load is not yet implemented",
        ))
    }

    /// Look up the versioned URL for `key`.
    pub fn resolve(&self, key: &str) -> KickResult<&str> {
        self.entries.get(key).map(String::as_str).ok_or_else(|| {
            KickError::new("RK_E_UNKNOWN_ASSET", format!("no asset for key `{}`", key))
                .with_hint("regenerate the manifest, or check the key spelling")
        })
    }

    /// Manifest format version surfaced by the build pipeline.
    pub fn version(&self) -> u32 {
        self.version
    }
}
