//! `cargo kick` — companion CLI for the [`kick-rs`](https://crates.io/crates/kick-rs)
//! framework. Currently ships:
//!
//! - `new <name>`                               — scaffold a fresh kick-rs project
//! - `g module <name>`                          — generate a module skeleton
//! - `g service <module>/<service_name>`        — generate a `#[service]`-derived stub
//! - `g contributor <module>/<contributor>`     — generate a `#[contributor]` async fn
//!
//! Future subcommands (`dev`, `add`, `info`, `check`) land in later
//! phases; see SPEC.md §7.
//!
//! Cargo subcommand convention: this binary is named `cargo-kick`, so
//! invoking `cargo kick <args>` runs us with `args[1] == "kick"`. We
//! strip that prefix below before handing off to clap.

use clap::{Parser, Subcommand};
use kick_rs_cli::{generate, new};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "cargo-kick",
    bin_name = "cargo kick",
    version,
    about = "Companion CLI for the kick-rs framework"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new kick-rs project.
    New {
        /// Project name. Used as the Cargo `package.name` and (snake-
        /// cased) as the Rust crate identifier. Must be lowercase ASCII
        /// letters / digits / `-` / `_`, starting with a letter.
        name: String,

        /// Directory to create the project in. Defaults to `./<name>`.
        #[arg(long)]
        path: Option<PathBuf>,

        /// Allow writing into a directory that already exists.
        /// Existing files inside are NOT removed.
        #[arg(long)]
        force: bool,
    },

    /// Generate code into an existing project (`g` is a shortcut).
    #[command(alias = "g")]
    Generate {
        #[command(subcommand)]
        kind: Generate,
    },
}

#[derive(Subcommand)]
enum Generate {
    /// Generate a new module skeleton (`mod.rs` + `handlers.rs`) and
    /// register it in `src/modules/mod.rs`.
    Module {
        /// Module name. Must be a Rust identifier: lowercase letters,
        /// digits, and underscores only (hyphens disallowed).
        name: String,

        /// Override the project root. Defaults to walking up from
        /// the current directory.
        #[arg(long)]
        path: Option<PathBuf>,

        /// Overwrite existing files inside the module directory.
        #[arg(long)]
        force: bool,
    },

    /// Generate a `#[service]`-derived stub inside an existing module.
    /// Spec is `<module>/<service_name>`, e.g. `users/email_sender`.
    Service {
        /// `<module>/<service_name>` spec (both halves must be valid
        /// snake_case identifiers).
        spec: String,

        /// Override the project root.
        #[arg(long)]
        path: Option<PathBuf>,

        /// Overwrite the service file if it already exists.
        #[arg(long)]
        force: bool,
    },

    /// Generate a `#[contributor]` async fn (plus a stub Output struct)
    /// inside an existing module. Spec is `<module>/<contributor_name>`,
    /// e.g. `users/load_current_user`.
    Contributor {
        /// `<module>/<contributor_name>` spec (both halves must be
        /// valid snake_case identifiers).
        spec: String,

        /// Override the project root.
        #[arg(long)]
        path: Option<PathBuf>,

        /// Overwrite the contributor file if it already exists.
        #[arg(long)]
        force: bool,
    },
}

fn main() -> ExitCode {
    // When invoked as `cargo kick new foo`, cargo runs us with argv
    // `["cargo-kick", "kick", "new", "foo"]`. Drop the redundant
    // "kick" so clap sees the real subcommand structure.
    let argv: Vec<String> = std::env::args()
        .enumerate()
        .filter(|(i, a)| !(*i == 1 && a == "kick"))
        .map(|(_, a)| a)
        .collect();

    let cli = match Cli::try_parse_from(argv) {
        Ok(c) => c,
        Err(e) => {
            // `e.print()` writes to the right stream (stdout for help,
            // stderr for errors) and `exit_code()` distinguishes them.
            let _ = e.print();
            return ExitCode::from(if e.exit_code() == 0 { 0 } else { 2 });
        }
    };

    match cli.command {
        Command::New { name, path, force } => {
            let args = new::NewArgs { name, path, force };
            match new::run(&args) {
                Ok(dest) => {
                    println!("✓ created kick-rs project at {}", dest.display());
                    println!("  next: cd {} && cargo run", dest.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Generate {
            kind: Generate::Module { name, path, force },
        } => {
            let args = generate::GenerateModuleArgs {
                name: name.clone(),
                project_root: path,
                force,
            };
            match generate::generate_module(&args) {
                Ok(dir) => {
                    println!("✓ generated module at {}", dir.display());
                    println!("  next: register it in main.rs via");
                    println!("        .module(modules::{name}::define())");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Generate {
            kind: Generate::Contributor { spec, path, force },
        } => {
            let args = generate::GenerateContributorArgs {
                spec: spec.clone(),
                project_root: path,
                force,
            };
            match generate::generate_contributor(&args) {
                Ok(file) => {
                    let (module, snake) = spec.split_once('/').unwrap();
                    let pascal = generate::to_pascal_case(snake);
                    println!("✓ generated contributor at {}", file.display());
                    println!("  next: register on the module's define() builder (or directly on");
                    println!("        bootstrap()) — in src/modules/{module}/mod.rs add");
                    println!("        use {snake}::{pascal};");
                    println!("        ...");
                    println!("        .contribute({pascal})");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Generate {
            kind: Generate::Service { spec, path, force },
        } => {
            let args = generate::GenerateServiceArgs {
                spec: spec.clone(),
                project_root: path,
                force,
            };
            match generate::generate_service(&args) {
                Ok(file) => {
                    // spec is validated by generate_service before file write, so
                    // splitting again here is safe.
                    let (module, service_snake) = spec.split_once('/').unwrap();
                    let pascal = generate::to_pascal_case(service_snake);
                    println!("✓ generated service at {}", file.display());
                    println!("  next: in src/modules/{module}/mod.rs, add");
                    println!("        use {service_snake}::{pascal};");
                    println!("        ...");
                    println!("        .service::<{pascal}>()");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
