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

## Status

Scaffolding-only today. Planned subcommands (`dev`, `g`, `add`,
`info`, `check`) land in later phases.

## License

MIT — see the workspace root.
