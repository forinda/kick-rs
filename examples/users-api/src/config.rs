//! Process configuration loaded via [`kick_rs::config::Config`].
//!
//! Layering: built-in defaults → optional `.env` file → `USERS_`-prefixed
//! process env. Final shape is deserialized into [`Env`].

use kick_rs::config::Config;
use kick_rs::KickResult;
use serde::Deserialize;

/// Process configuration read at startup.
#[derive(Debug, Clone, Deserialize)]
pub struct Env {
    pub database_url: String,
    pub bind_addr: String,
}

impl Env {
    /// Build the loader, apply defaults, optionally pick up a local
    /// `.env`, then override from `USERS_*` env vars.
    pub fn load() -> KickResult<Self> {
        Config::builder()
            .with_defaults(serde_json::json!({
                "bind_addr": "0.0.0.0:3000",
            }))
            .with_dotenv_optional(".env")
            .with_env_prefix("USERS_")
            .extract()
    }
}
