//! `cargo kick` — companion CLI for the [`kick-rs`](https://crates.io/crates/kick-rs)
//! framework. Currently ships one subcommand:
//!
//! - `new <name>` — scaffold a fresh kick-rs project
//!
//! Future subcommands (`dev`, `g`, `add`, `info`, `check`) land in
//! later phases; see SPEC.md §7.
//!
//! Cargo subcommand convention: this binary is named `cargo-kick`, so
//! invoking `cargo kick <args>` runs us with `args[1] == "kick"`. We
//! strip that prefix below before handing off to clap.

use clap::{Parser, Subcommand};
use kick_rs_cli::new;
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
    }
}
