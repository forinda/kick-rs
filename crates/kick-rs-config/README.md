# kick-rs-config

> Layered, typed configuration loader for [kick-rs](https://github.com/forinda/kick-rs).

[![crates.io](https://img.shields.io/crates/v/kick-rs-config.svg)](https://crates.io/crates/kick-rs-config)
[![docs.rs](https://docs.rs/kick-rs-config/badge.svg)](https://docs.rs/kick-rs-config)
[![license](https://img.shields.io/crates/l/kick-rs-config.svg)](https://github.com/forinda/kick-rs/blob/main/LICENSE)

Defaults → file → dotenv → process env, merged into a single
`serde_json::Value` and deserialized into your struct.

```rust
use kick_rs_config::Config;
use serde::Deserialize;

#[derive(Deserialize)]
struct AppConfig {
    port: u16,
    database_url: String,
}

let cfg: AppConfig = Config::builder()
    .with_defaults(serde_json::json!({ "port": 3000 }))
    .with_toml_file_optional("config.toml")
    .with_dotenv_optional(".env")
    .with_env_prefix("APP_")
    .extract()?;
```

Subsequent sources deep-merge over previous ones. Inside `with_env_prefix`,
`__` (double underscore) becomes a path separator, so `APP_DB__URL`
populates `db.url`.

## Why not figment / config-rs?

Those are excellent crates. `kick-rs-config` wraps a tighter surface
focused on `kick-rs` idioms — `KickError`-typed failures, JSON-shaped
intermediate representation, no profile magic. Adopters with more
exotic needs can keep using figment directly and pass the result
into the container via `bootstrap().service_value(cfg)`.

## Install

```toml
[dependencies]
kick-rs-config = "0.1.0-alpha.1"

# or via the umbrella crate with the `config` feature:
kick-rs = { version = "0.1.0-alpha.1", features = ["config"] }
```

## License

MIT — see the workspace root.
