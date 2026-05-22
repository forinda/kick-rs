//! # kick-rs-config
//!
//! Layered, typed configuration loader for `kick-rs`.
//!
//! Sources stack in declaration order; each source deep-merges over
//! the previous one. Final shape is deserialized via `serde` into the
//! target type:
//!
//! ```no_run
//! use kick_rs_config::Config;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct AppConfig {
//!     port: u16,
//!     database_url: String,
//! }
//!
//! let cfg: AppConfig = Config::builder()
//!     .with_defaults(serde_json::json!({ "port": 3000 }))
//!     .with_toml_file_optional("config.toml")
//!     .with_dotenv_optional(".env")
//!     .with_env_prefix("APP_")
//!     .extract()
//!     .expect("config");
//! ```
//!
//! ## Source semantics
//!
//! | Source                   | Read order  | Optional? | Notes                                          |
//! |--------------------------|-------------|-----------|------------------------------------------------|
//! | `with_defaults(json)`    | first       | n/a       | Static fallback layer                          |
//! | `with_toml_file{,_opt}`  | next        | opt       | TOML mapped to JSON for merging                |
//! | `with_json_file{,_opt}`  | next        | opt       | JSON file                                      |
//! | `with_dotenv{,_opt}`     | next        | opt       | `KEY=VALUE` lines populate `std::env`          |
//! | `with_env_prefix(p)`     | last        | n/a       | Reads `std::env` with `p` prefix; overrides    |
//!
//! Later sources win. Within `with_env_prefix`, `__` (double underscore)
//! is interpreted as a nesting separator (e.g. `APP_DB__URL` →
//! `db.url`).
//!
//! ## Why not figment / config-rs?
//!
//! Those are excellent crates. `kick-rs-config` deliberately wraps a
//! tighter surface focused on the kick-rs idioms — `KickError`-typed
//! failures, JSON-shaped intermediate representation, no profile
//! magic. Adopters with more exotic needs can keep using figment
//! directly and pass the result into the container via
//! `bootstrap().service_value(cfg)`.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

use kick_rs_core::{KickError, KickResult};
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

mod env;
mod merge;

pub use merge::deep_merge;

/// Layered, typed configuration. Build via [`Config::builder`].
///
/// Holds the final merged JSON value. Use [`Config::extract`] (or the
/// shorthand on the builder) to deserialize into a typed struct.
#[derive(Debug, Clone)]
pub struct Config {
    merged: serde_json::Value,
}

impl Config {
    /// Start a new [`ConfigBuilder`] with an empty source list.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    /// The merged JSON view of all sources, for inspection.
    pub fn raw(&self) -> &serde_json::Value {
        &self.merged
    }

    /// Deserialize the merged config into a typed struct.
    ///
    /// Failures map to `RK_C_DESERIALIZE` with the inner serde error
    /// message so adopters can see which field broke.
    pub fn extract<T: DeserializeOwned>(&self) -> KickResult<T> {
        serde_json::from_value(self.merged.clone()).map_err(|e| {
            KickError::new(
                "RK_C_DESERIALIZE",
                format!("could not deserialize merged config: {e}"),
            )
        })
    }
}

/// Builder for a [`Config`]. Sources are applied in the order they're
/// added; later sources win on conflicting keys.
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    sources: Vec<Source>,
}

#[derive(Debug)]
enum Source {
    Defaults(serde_json::Value),
    TomlFile { path: PathBuf, optional: bool },
    JsonFile { path: PathBuf, optional: bool },
    DotenvFile { path: PathBuf, optional: bool },
    EnvPrefix { prefix: String },
}

impl ConfigBuilder {
    /// Static defaults layer. Anything not overridden by later sources
    /// keeps its default.
    pub fn with_defaults(mut self, defaults: serde_json::Value) -> Self {
        self.sources.push(Source::Defaults(defaults));
        self
    }

    /// Load a TOML file. Errors if the file is missing or malformed.
    pub fn with_toml_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(Source::TomlFile {
            path: path.into(),
            optional: false,
        });
        self
    }

    /// Load a TOML file *if it exists*. Missing-file is fine; a present
    /// but malformed file still errors.
    pub fn with_toml_file_optional(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(Source::TomlFile {
            path: path.into(),
            optional: true,
        });
        self
    }

    /// Load a JSON file. Errors if the file is missing or malformed.
    pub fn with_json_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(Source::JsonFile {
            path: path.into(),
            optional: false,
        });
        self
    }

    /// Load a JSON file *if it exists*.
    pub fn with_json_file_optional(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(Source::JsonFile {
            path: path.into(),
            optional: true,
        });
        self
    }

    /// Load a `.env`-style file (`KEY=VALUE`, `#` comments). Parsed
    /// entries are inserted into `std::env::set_var` so that subsequent
    /// [`Self::with_env_prefix`] sources see them.
    ///
    /// This crate is the only place we touch `std::env` for you. Other
    /// sources are pure functions of the file system + the
    /// already-existing process environment.
    pub fn with_dotenv(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(Source::DotenvFile {
            path: path.into(),
            optional: false,
        });
        self
    }

    /// Load a dotenv file *if it exists*.
    pub fn with_dotenv_optional(mut self, path: impl Into<PathBuf>) -> Self {
        self.sources.push(Source::DotenvFile {
            path: path.into(),
            optional: true,
        });
        self
    }

    /// Read environment variables with the given prefix. The prefix is
    /// stripped and the remainder lowercased. `__` becomes a path
    /// separator for nested keys.
    ///
    /// Example: `APP_DB__URL=postgres://...` with prefix `"APP_"`
    /// produces the JSON pointer `/db/url`.
    pub fn with_env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.sources.push(Source::EnvPrefix {
            prefix: prefix.into(),
        });
        self
    }

    /// Finalize the builder.
    pub fn build(self) -> KickResult<Config> {
        let mut merged = serde_json::Value::Object(serde_json::Map::new());
        for source in &self.sources {
            if let Some(layer) = read_source(source)? {
                deep_merge(&mut merged, layer);
            }
        }
        Ok(Config { merged })
    }

    /// Convenience: build + extract in one call.
    pub fn extract<T: DeserializeOwned>(self) -> KickResult<T> {
        self.build()?.extract()
    }
}

fn read_source(source: &Source) -> KickResult<Option<serde_json::Value>> {
    match source {
        Source::Defaults(v) => Ok(Some(v.clone())),
        Source::TomlFile { path, optional } => read_toml(path, *optional),
        Source::JsonFile { path, optional } => read_json(path, *optional),
        Source::DotenvFile { path, optional } => {
            env::load_dotenv(path, *optional)?;
            // Dotenv only populates the process env; the actual values
            // get picked up by a later EnvPrefix source.
            Ok(None)
        }
        Source::EnvPrefix { prefix } => Ok(Some(env::read_env_with_prefix(prefix))),
    }
}

fn read_toml(path: &Path, optional: bool) -> KickResult<Option<serde_json::Value>> {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let parsed: toml::Value = toml::from_str(&s).map_err(|e| {
                KickError::new(
                    "RK_C_PARSE",
                    format!("invalid TOML in {}: {e}", path.display()),
                )
            })?;
            let v = serde_json::to_value(parsed).map_err(|e| {
                KickError::new(
                    "RK_C_PARSE",
                    format!("could not normalize TOML from {}: {e}", path.display()),
                )
            })?;
            Ok(Some(v))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && optional => Ok(None),
        Err(e) => Err(KickError::new(
            "RK_C_IO",
            format!("could not read {}: {e}", path.display()),
        )),
    }
}

fn read_json(path: &Path, optional: bool) -> KickResult<Option<serde_json::Value>> {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| {
                KickError::new(
                    "RK_C_PARSE",
                    format!("invalid JSON in {}: {e}", path.display()),
                )
            })?;
            Ok(Some(v))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && optional => Ok(None),
        Err(e) => Err(KickError::new(
            "RK_C_IO",
            format!("could not read {}: {e}", path.display()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct AppConfig {
        port: u16,
        database_url: String,
    }

    #[test]
    fn defaults_only() {
        let cfg: AppConfig = Config::builder()
            .with_defaults(serde_json::json!({
                "port": 3000,
                "database_url": "postgres://localhost/dev",
            }))
            .extract()
            .unwrap();
        assert_eq!(cfg.port, 3000);
        assert_eq!(cfg.database_url, "postgres://localhost/dev");
    }

    #[test]
    fn missing_required_file_errors() {
        let err = Config::builder()
            .with_toml_file("does-not-exist.toml")
            .build()
            .unwrap_err();
        assert_eq!(err.code, "RK_C_IO");
    }

    #[test]
    fn missing_optional_file_is_fine() {
        let cfg = Config::builder()
            .with_defaults(serde_json::json!({ "port": 1 }))
            .with_toml_file_optional("does-not-exist.toml")
            .build()
            .unwrap();
        assert_eq!(cfg.raw()["port"], 1);
    }

    #[test]
    fn layering_is_defaults_lt_file_lt_env() {
        use std::io::Write as _;

        let toml_file = tempfile::NamedTempFile::new().unwrap();
        write!(
            toml_file.as_file(),
            r#"port = 8080
database_url = "postgres://from-file"
"#
        )
        .unwrap();

        std::env::set_var("LAYER_DATABASE_URL", "postgres://from-env");

        let cfg: AppConfig = Config::builder()
            .with_defaults(serde_json::json!({
                "port": 3000,
                "database_url": "postgres://from-defaults",
            }))
            .with_toml_file(toml_file.path())
            .with_env_prefix("LAYER_")
            .extract()
            .unwrap();

        assert_eq!(cfg.port, 8080, "file overrides defaults");
        assert_eq!(
            cfg.database_url, "postgres://from-env",
            "env overrides file"
        );

        std::env::remove_var("LAYER_DATABASE_URL");
    }

    #[test]
    fn deserialize_error_is_useful() {
        let err = Config::builder()
            .with_defaults(serde_json::json!({
                "port": "not-a-number",
                "database_url": "x",
            }))
            .extract::<AppConfig>()
            .unwrap_err();
        assert_eq!(err.code, "RK_C_DESERIALIZE");
        // serde's invalid-type message mentions the problem field name
        // somewhere in the error string.
        assert!(
            err.message.contains("port") || err.message.contains("u16"),
            "got error: {}",
            err.message
        );
    }
}
