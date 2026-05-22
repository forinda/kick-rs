//! Process configuration loaded via [`kick_rs::config::Config`].
//!
//! Same shape as the users-api example, plus a `tenants` allowlist —
//! sourced as a comma-separated env value and split during
//! deserialization.

use kick_rs::config::Config;
use kick_rs::{KickError, KickResult};
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Deserialize)]
pub struct Env {
    pub database_url: String,
    pub bind_addr: String,
    /// Allowlist of tenant slugs. Env value is comma-separated; we
    /// split + trim during deserialization. Empty after split-and-trim
    /// is treated as a configuration error post-load.
    #[serde(deserialize_with = "csv_to_vec")]
    pub tenants: Vec<String>,
}

impl Env {
    pub fn load() -> KickResult<Self> {
        let cfg: Self = Config::builder()
            .with_defaults(serde_json::json!({
                "bind_addr": "0.0.0.0:3001",
            }))
            .with_dotenv_optional(".env")
            .with_env_prefix("MT_")
            .extract()?;

        if cfg.tenants.is_empty() {
            return Err(KickError::new(
                "RK_A_CONFIG",
                "MT_TENANTS must list at least one tenant slug",
            ));
        }
        Ok(cfg)
    }
}

fn csv_to_vec<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(d)?;
    Ok(s.split(',')
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect())
}
