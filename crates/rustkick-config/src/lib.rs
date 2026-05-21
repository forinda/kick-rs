//! # rustkick-config
//!
//! Env-driven config loader. Mirrors KickJS `defineEnv` / `loadEnv` /
//! `ConfigService`. Real implementation lands in Phase 3 — this file
//! reserves the public surface.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

use rustkick_core::{KickError, KickResult};

/// Reads and validates an env struct. Single source of truth for typed
/// process configuration; bound into the [`Container`](rustkick_core::Container)
/// as a singleton.
pub struct ConfigService {
    // Real shape lands in Phase 3.
}

impl ConfigService {
    /// Read a key as a string. Returns `RK_C_MISSING_ENV` if absent.
    pub fn get(&self, _key: &str) -> KickResult<String> {
        Err(KickError::new(
            "RK_E_UNIMPLEMENTED",
            "ConfigService::get is not yet implemented",
        ))
    }
}
