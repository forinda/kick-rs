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

### `cargo kick g contributor <module>/<name>`

Generates a `#[contributor]`-derived stub inside an existing module:

```bash
cd my-app
cargo kick g contributor users/load_current_user
# ✓ generated contributor at .../src/modules/users/load_current_user.rs
#   next: register on the module's define() builder (or directly on
#         bootstrap()) — in src/modules/users/mod.rs add
#         use load_current_user::LoadCurrentUser;
#         ...
#         .contribute(LoadCurrentUser)
```

Emits `src/modules/<module>/<name>.rs` with a `#[contributor]` async
fn (PascalCase name derived from the snake_case file) and a stub
`<Name>Out` output struct. Idempotently appends `pub mod <name>;` to
the parent module's `mod.rs`.

Flags:

- `--path <PATH>` — override project-root detection.
- `--force` — overwrite the contributor file if it already exists.

## Auto-registration

By default each `g` subcommand also writes the wiring needed to use
the generated code:

| Generator        | Edits                                                                   |
|------------------|-------------------------------------------------------------------------|
| `g module`       | Inserts `.module(modules::<name>::define())` into `src/main.rs`         |
| `g service`      | Inserts `use <name>::<Pascal>;` + `.service::<Pascal>()` in parent `mod.rs` |
| `g contributor`  | Inserts `use <name>::<Pascal>;` + `.contribute(Pascal)` in parent `mod.rs` |

Insertion is conservative and text-level (no `syn` round-trip), so
formatting, comments, and blank-line layout are preserved. The
target file must use the known patterns (`bootstrap()` chain,
`define_module(...)` chain) — if not, the CLI prints the exact
snippet to paste and leaves the file untouched.

Each `g` subcommand accepts a `--no-register` flag to skip the
insertion if you'd rather wire things up yourself.

## Status

`new`, `g module`, `g service`, `g contributor` today (with
auto-registration). Planned: `dev`, `add`, `info`, `check`.

## License

MIT — see the workspace root.
