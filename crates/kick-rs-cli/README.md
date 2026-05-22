# kick-rs-cli

> `cargo kick` — companion CLI for the
> [kick-rs](https://crates.io/crates/kick-rs) framework.

[![crates.io](https://img.shields.io/crates/v/kick-rs-cli.svg)](https://crates.io/crates/kick-rs-cli)
[![license](https://img.shields.io/crates/l/kick-rs-cli.svg)](https://github.com/forinda/kick-rs/blob/main/LICENSE)

## Install

```bash
cargo install kick-rs-cli
```

This installs a `cargo-kick` binary, which cargo picks up as the
`kick` subcommand.

## Usage

### `cargo kick new <name>`

Scaffolds a new kick-rs project with a working `hello` module:

```bash
cargo kick new my-app
cd my-app
cargo run
# server starts on 0.0.0.0:3000

curl http://localhost:3000/hello
# {"message":"Hello from kick-rs!"}
```

Generated layout:

```
my-app/
├── Cargo.toml               # depends on kick-rs (macros + config features)
├── README.md                # quick run instructions
├── .env.example
├── .gitignore
└── src/
    ├── main.rs              # bootstrap() + tokio runtime
    └── modules/
        ├── mod.rs
        └── hello/
            ├── mod.rs       # define_module(...) -> Module
            └── handlers.rs  # greet, greet_named
```

Flags:

- `--path <PATH>` — write into a different directory (defaults to `./<name>`).
- `--force` — write into an existing directory. Existing files inside are NOT removed.

### `cargo kick g module <name>`

Generates a new module skeleton inside an existing kick-rs project:

```bash
cd my-app
cargo kick g module posts
# ✓ generated module at .../my-app/src/modules/posts
#   next: register it in main.rs via
#         .module(modules::posts::define())
```

Emits `src/modules/<name>/{mod.rs,handlers.rs}` and idempotently
appends `pub mod <name>;` to `src/modules/mod.rs`. The project root
is auto-detected by walking up from `cwd` until `src/modules/mod.rs`
is found.

Module names must be valid Rust identifiers: lowercase letters /
digits / underscores, starting with a letter. Hyphens are
rejected (Rust modules can't have them).

Flags:

- `--path <PATH>` — override project-root detection.
- `--force` — overwrite existing files inside the module directory.

### `cargo kick g service <module>/<name>`

Generates a `#[service]`-derived stub inside an existing module:

```bash
cd my-app
cargo kick g service users/email_sender
# ✓ generated service at .../src/modules/users/email_sender.rs
#   next: in src/modules/users/mod.rs, add
#         use email_sender::EmailSender;
#         ...
#         .service::<EmailSender>()
```

Emits `src/modules/<module>/<name>.rs` containing a `#[service]`
struct (PascalCase derived from the snake_case file name) and
idempotently appends `pub mod <name>;` to that module's `mod.rs`.

Both halves of the spec must be valid snake_case identifiers. The
parent module must already exist (use `g module` first).

Flags:

- `--path <PATH>` — override project-root detection.
- `--force` — overwrite the service file if it already exists.

## Status

`new`, `g module`, `g service` today. Planned: `g contributor`,
`dev`, `add`, `info`, `check`.

## License

MIT — see the workspace root.
