//! `cargo kick` — companion CLI for the [`kick-rs`](https://crates.io/crates/kick-rs)
//! framework. Currently ships:
//!
//! - `new <name>`                               — scaffold a fresh kick-rs project
//! - `g module <name>`                          — generate a module skeleton
//! - `g service <module>/<service_name>`        — generate a `#[service]`-derived stub
//! - `g contributor <module>/<contributor>`     — generate a `#[contributor]` async fn
//! - `add <feature>`                            — toggle an opt-in `kick-rs` feature in Cargo.toml
//! - `info`                                     — print a snapshot of the current project
//! - `dev`                                      — watch the source tree and restart on save
//!
//! Future subcommands (`check`) land in later phases; see SPEC.md §7.
//!
//! Cargo subcommand convention: this binary is named `cargo-kick`, so
//! invoking `cargo kick <args>` runs us with `args[1] == "kick"`. We
//! strip that prefix below before handing off to clap.

use clap::{Parser, Subcommand};
use kick_rs_cli::register::RegisterOutcome;
use kick_rs_cli::{add, dev, generate, info, new};
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

    /// Watch the source tree and restart `cargo run` on save.
    /// Wraps `cargo run` with a debounced file watcher rooted at
    /// `src/`. Editor saves trigger a kill+respawn; stdout/stderr
    /// from the child stream through unchanged.
    Dev {
        /// Override the project root.
        #[arg(long)]
        path: Option<PathBuf>,

        /// Extra directories to watch (in addition to `src/`).
        /// Repeatable: `--watch templates --watch fixtures`.
        #[arg(long = "watch")]
        watch: Vec<PathBuf>,

        /// Debounce window in milliseconds. Defaults to 250.
        #[arg(long, default_value_t = 250)]
        debounce_ms: u64,
    },

    /// Print a snapshot of the current project — package version,
    /// kick-rs dep version + features, and every module on disk with
    /// the services and contributors registered on each.
    Info {
        /// Override the project root.
        #[arg(long)]
        path: Option<PathBuf>,

        /// Dep name to inspect (defaults to `kick-rs`).
        #[arg(long, default_value = "kick-rs")]
        dep_name: String,
    },

    /// Toggle an opt-in `kick-rs` feature on the umbrella dep in
    /// Cargo.toml. Pass `list` to print the known features.
    Add {
        /// Feature to enable. Pass `list` to print known features and exit.
        feature: String,

        /// Override the project root.
        #[arg(long)]
        path: Option<PathBuf>,

        /// Dependency name to mutate (defaults to `kick-rs`). Useful
        /// when you've renamed the umbrella in a workspace.
        #[arg(long, default_value = "kick-rs")]
        dep_name: String,
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

        /// Skip the `.module(...)` insertion into `src/main.rs`. Use
        /// when your bootstrap lives outside `main.rs` and you want to
        /// register manually.
        #[arg(long)]
        no_register: bool,
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

        /// Skip the `use` + `.service::<...>()` insertion into the
        /// parent module's `mod.rs`.
        #[arg(long)]
        no_register: bool,
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

        /// Skip the `use` + `.contribute(...)` insertion into the
        /// parent module's `mod.rs`.
        #[arg(long)]
        no_register: bool,
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
            kind:
                Generate::Module {
                    name,
                    path,
                    force,
                    no_register,
                },
        } => {
            let args = generate::GenerateModuleArgs {
                name: name.clone(),
                project_root: path,
                force,
                auto_register: !no_register,
            };
            match generate::generate_module(&args) {
                Ok(res) => {
                    println!("✓ generated module at {}", res.module_dir.display());
                    print_register_outcome(
                        &res.register,
                        "main.rs",
                        &format!(".module(modules::{name}::define())"),
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Generate {
            kind:
                Generate::Service {
                    spec,
                    path,
                    force,
                    no_register,
                },
        } => {
            let args = generate::GenerateServiceArgs {
                spec: spec.clone(),
                project_root: path,
                force,
                auto_register: !no_register,
            };
            match generate::generate_service(&args) {
                Ok(res) => {
                    let (module, service_snake) = spec.split_once('/').unwrap();
                    let pascal = generate::to_pascal_case(service_snake);
                    println!("✓ generated service at {}", res.file.display());
                    print_register_outcome(
                        &res.register,
                        &format!("src/modules/{module}/mod.rs"),
                        &format!(
                            "use {service_snake}::{pascal};\n        ...\n        .service::<{pascal}>()"
                        ),
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Generate {
            kind:
                Generate::Contributor {
                    spec,
                    path,
                    force,
                    no_register,
                },
        } => {
            let args = generate::GenerateContributorArgs {
                spec: spec.clone(),
                project_root: path,
                force,
                auto_register: !no_register,
            };
            match generate::generate_contributor(&args) {
                Ok(res) => {
                    let (module, snake) = spec.split_once('/').unwrap();
                    let pascal = generate::to_pascal_case(snake);
                    println!("✓ generated contributor at {}", res.file.display());
                    print_register_outcome(
                        &res.register,
                        &format!("src/modules/{module}/mod.rs"),
                        &format!(
                            "use {snake}::{pascal};\n        ...\n        .contribute({pascal})"
                        ),
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Dev {
            path,
            watch,
            debounce_ms,
        } => {
            let args = dev::DevArgs {
                project_root: path,
                watch_paths: watch,
                debounce_ms,
            };
            match dev::run(&args) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Info { path, dep_name } => {
            let args = info::InfoArgs {
                project_root: path,
                dep_name,
            };
            match info::collect_info(&args) {
                Ok(snapshot) => {
                    print!("{}", info::render_info(&snapshot));
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Add {
            feature,
            path,
            dep_name,
        } => {
            // `add list` is a help shortcut — print the catalog and exit.
            if feature == "list" {
                println!("kick-rs features that `cargo kick add` knows about:");
                for (name, desc) in add::KNOWN_FEATURES {
                    println!("  {name:10} — {desc}");
                }
                return ExitCode::SUCCESS;
            }

            let args = add::AddArgs {
                feature: feature.clone(),
                project_root: path,
                dep_name: dep_name.clone(),
            };
            match add::add_feature(&args) {
                Ok(add::AddOutcome::Added) => {
                    println!("✓ added `{feature}` to {dep_name} features in Cargo.toml");
                    ExitCode::SUCCESS
                }
                Ok(add::AddOutcome::AlreadyEnabled) => {
                    println!("· `{feature}` already enabled on {dep_name}");
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

/// Surface the auto-register outcome to the user as a short hint.
/// Inserted/AlreadyRegistered get a one-liner; otherwise we print the
/// manual snippet so adopters can paste it themselves.
fn print_register_outcome(outcome: &RegisterOutcome, target_path: &str, manual_snippet: &str) {
    match outcome {
        RegisterOutcome::Inserted => {
            println!("  ✓ registered in {target_path}");
        }
        RegisterOutcome::AlreadyRegistered => {
            println!("  · {target_path} already had the registration — no edit needed");
        }
        RegisterOutcome::TargetMissing => {
            println!("  ! {target_path} not found — add manually:");
            println!("        {manual_snippet}");
        }
        RegisterOutcome::AnchorNotFound => {
            println!("  ! could not find a known builder pattern in {target_path}; add manually:");
            println!("        {manual_snippet}");
        }
        RegisterOutcome::Skipped => {
            println!("  · skipped per --no-register — add manually:");
            println!("        {manual_snippet}");
        }
    }
}
