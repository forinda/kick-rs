//! Env-driven config. This example does its own loading rather than
//! depending on `rustkick-config` (still scaffold-only at the time of
//! writing) — keeps the example self-contained.

use rustkick::{KickError, KickResult};

/// Process configuration read from environment variables at startup.
#[derive(Debug, Clone)]
pub struct Env {
    pub database_url: String,
    pub bind_addr: String,
}

impl Env {
    /// Read environment variables, applying defaults where reasonable.
    pub fn load() -> KickResult<Self> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_owned()),
        })
    }
}

fn required(key: &'static str) -> KickResult<String> {
    std::env::var(key).map_err(|_| {
        KickError::new("RK_C_MISSING_ENV", format!("required env var `{key}` is not set"))
            .with_hint(format!("export {key}=... or copy .env.example to .env"))
            .with_context("key", key)
    })
}
