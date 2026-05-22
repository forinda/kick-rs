//! Env-driven config — same shape as the users-api example, plus a
//! `TENANTS` allowlist.

use kick_rs::{KickError, KickResult};

#[derive(Debug, Clone)]
pub struct Env {
    pub database_url: String,
    pub bind_addr: String,
    pub tenants: Vec<String>,
}

impl Env {
    pub fn load() -> KickResult<Self> {
        let tenants = required("TENANTS")?
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if tenants.is_empty() {
            return Err(KickError::new(
                "RK_A_CONFIG",
                "TENANTS env must list at least one tenant slug",
            ));
        }
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3001".to_owned()),
            tenants,
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
