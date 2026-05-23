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

```text
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

### `cargo kick add <feature>`

Toggle an opt-in `kick-rs` feature on the umbrella dep in your
project's `Cargo.toml`:

```bash
cargo kick add openapi
# ✓ added `openapi` to kick-rs features in Cargo.toml

cargo kick add openapi
# · `openapi` already enabled on kick-rs

cargo kick add list
# kick-rs features that `cargo kick add` knows about:
#   macros     — `#[service]`, `#[contributor]`, `#[get]`/`#[post]`/...
#   config     — Layered env / dotenv / TOML / JSON config loader
#   openapi    — OpenApiPlugin + paths!() — serve /openapi.json
#   devtools   — /__debug introspection endpoint (also needs .with_devtools())
```

The mutation uses `toml_edit` so the rest of your Cargo.toml
(comments, alignment, key order) is left untouched. A bare
`kick-rs = "0.1"` is promoted to the inline-table form
`kick-rs = { version = "0.1", features = ["..."] }` so the
features array has a place to live.

Flags:

- `--path <PATH>` — override project-root detection.
- `--dep-name <NAME>` — mutate a differently-named dep (rare; defaults to `kick-rs`).

### `cargo kick info`

Print a snapshot of the current kick-rs project — package version,
the `kick-rs` dep version + enabled features, and every module on
disk with the services and contributors registered on each:

```bash
$ cargo kick info
kick-rs project: my-app 0.1.0
  root:        /path/to/my-app
  kick-rs dep: 0.1.0
  features:    macros, config, openapi

modules (2):
  - hello (prefix /hello)
  - posts (prefix /posts)
      services:     PostService
      contributors: LoadPost
```

Module detail is parsed from the same `define_module(...)`,
`.prefix(...)`, `.service::<...>()`, `.contribute(...)` patterns
the scaffold + generators emit. Custom builder wrappers fall back to
a directory-name-only entry.

Flags:

- `--path <PATH>` — override project-root detection.
- `--dep-name <NAME>` — inspect a renamed dep (defaults to `kick-rs`).

### `cargo kick dev`

Watch the project's source tree and restart `cargo run` on save:

```bash
cargo kick dev
# cargo kick dev — starting initial run in `/path/to/my-app`
#   watching /path/to/my-app/src
#   Ctrl-C to quit.
#
# <cargo's normal output — compile, run, stdout/stderr of your app>
#
# (touch src/main.rs)
#
# cargo kick dev — change detected; restarting
```

Files trigger a restart when they match Rust source / TOML / common
template extensions, *and* aren't inside `target/`, `.git/`,
`node_modules/`, or any editor temp file (`~`-suffixed). Events are
debounced 250ms (configurable) so editor save storms produce one
restart, not N.

Flags:

- `--path <PATH>` — override project-root detection.
- `--watch <DIR>` — extra directories to watch (repeatable; useful
  for `templates/`, `static/`, etc).
- `--debounce-ms <MS>` — debounce window, default `250`.

**Process-tree cleanup:** cargo spawns the built binary as its
grandchild. `kick dev` puts cargo in its own process group on
spawn (Unix `setpgid` / Windows `CREATE_NEW_PROCESS_GROUP`) and
sends the kill signal to the whole group on restart (`kill -KILL
-<pgid>` on Unix, `taskkill /F /T /PID` on Windows) so the bound
port releases immediately. No extra deps — both platforms use the
OS's bundled kill utility.

### `cargo kick check`

Lint a kick-rs project for common misconfigurations the compiler
doesn't catch. Useful as a CI gate after running generators.

```bash
cargo kick check
# kick-rs check: ✓ clean
# (exit 0)

# … or, when something's wrong:
# kick-rs check: 1 finding(s)
#
#   [RK_K_UNMOUNTED_MODULE] module `orphan` is declared in src/modules/mod.rs
#       but not mounted in src/main.rs (expected `.module(modules::orphan::define())`)
#       → /path/to/my-app/src/main.rs
# (exit 1)
```

Lints today:

| Code                              | What it catches                                              |
|-----------------------------------|--------------------------------------------------------------|
| `RK_K_UNMOUNTED_MODULE`           | `pub mod X;` exists but `main.rs` doesn't call `.module(modules::X::define())` |
| `RK_K_STALE_PUB_MOD`              | `pub mod X;` whose `src/modules/X/` directory (or `.rs` file) doesn't exist |
| `RK_K_UNREGISTERED_SERVICE`       | `#[service] pub struct Foo` in a file but no `.service::<Foo>()` in parent `mod.rs` |
| `RK_K_UNREGISTERED_CONTRIBUTOR`   | `#[contributor] pub async fn Foo` in a file but no `.contribute(Foo)` in parent `mod.rs` |

Pure-text scanners — no `cargo check` invocation, no `syn` parse.
Recognizes the shapes the scaffold + `cargo kick g` produce; exotic
hand-written wiring may slip through.

Flags:

- `--path <PATH>` — override project-root detection.

## Status

`new`, `g module`, `g service`, `g contributor`, `add`, `info`,
`dev`, `check` today. Everything from the SPEC's CLI surface is
shipped.

## License

MIT — see the workspace root.
