//! `cargo kick dev` — watch the project's source tree and restart
//! the app on save.
//!
//! Thin wrapper over `cargo run`. On each batch of debounced file
//! events under `src/` (or any user-supplied path), we kill the
//! current child and respawn `cargo run`. stdout/stderr from the
//! child stream through to the user's terminal so compile errors
//! and runtime logs land as they would for a manual `cargo run`.
//!
//! Cross-platform caveat: `Child::kill()` terminates the immediate
//! `cargo` process but won't always reap grandchildren (the built
//! app's process) on Windows. If your app holds a port, the next
//! restart may briefly see an `EADDRINUSE` while the OS reaps it.
//! Investing in `taskkill /T` or `shared_child` is a follow-up.

use crate::generate::{find_project_root, GenerateError};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;

/// Decoded form of the `dev` subcommand.
pub struct DevArgs {
    /// Override the project root. Defaults to walking up from `cwd`.
    pub project_root: Option<PathBuf>,
    /// Extra paths to watch (in addition to `src/`). Useful for
    /// templates, static fixtures, anything that should trigger a
    /// rebuild but doesn't live under `src/`. Defaults to empty.
    pub watch_paths: Vec<PathBuf>,
    /// Debounce window for file events. Defaults to 250ms — long
    /// enough to swallow the multi-event storm editors emit on save,
    /// short enough that adopters don't notice the lag.
    pub debounce_ms: u64,
}

impl Default for DevArgs {
    fn default() -> Self {
        Self {
            project_root: None,
            watch_paths: Vec::new(),
            debounce_ms: 250,
        }
    }
}

#[derive(Debug)]
pub enum DevError {
    ProjectRoot(GenerateError),
    Watcher(notify::Error),
    Io { path: PathBuf, source: io::Error },
    CargoSpawn(io::Error),
}

impl std::fmt::Display for DevError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProjectRoot(e) => write!(f, "{e}"),
            Self::Watcher(e) => write!(f, "could not set up file watcher: {e}"),
            Self::Io { path, source } => write!(f, "I/O error at `{}`: {source}", path.display()),
            Self::CargoSpawn(e) => write!(f, "could not spawn `cargo run`: {e}"),
        }
    }
}

impl std::error::Error for DevError {}

/// Run the dev loop. Returns only when the user Ctrl-C's the parent
/// process — the watcher otherwise loops forever.
pub fn run(args: &DevArgs) -> Result<(), DevError> {
    let root = match &args.project_root {
        Some(p) => p.clone(),
        None => find_project_root(Path::new(".")).map_err(DevError::ProjectRoot)?,
    };

    // Initial spawn — fail fast if `cargo` isn't on PATH.
    eprintln!(
        "cargo kick dev — starting initial run in `{}`",
        root.display()
    );
    let mut child = spawn_cargo_run(&root)?;

    // notify-debouncer-mini coalesces event storms into one
    // `Vec<DebouncedEvent>` per debounce window. The channel
    // receives those vecs; one vec = one rebuild trigger.
    let (tx, rx) = channel();
    let mut debouncer = new_debouncer(Duration::from_millis(args.debounce_ms), move |res| {
        // We pass the Result through unchanged — the loop below
        // logs errors but keeps watching.
        let _ = tx.send(res);
    })
    .map_err(DevError::Watcher)?;

    let watcher = debouncer.watcher();

    // Always watch `src/`. Adopter-supplied extras come next.
    let src = root.join("src");
    watcher
        .watch(&src, RecursiveMode::Recursive)
        .map_err(DevError::Watcher)?;
    eprintln!("  watching {}", src.display());
    for extra in &args.watch_paths {
        let abs = if extra.is_absolute() {
            extra.clone()
        } else {
            root.join(extra)
        };
        watcher
            .watch(&abs, RecursiveMode::Recursive)
            .map_err(DevError::Watcher)?;
        eprintln!("  watching {}", abs.display());
    }

    eprintln!("  Ctrl-C to quit.\n");

    // Main loop: every time the debounce window yields events,
    // kill the current child and respawn. Idle times use a short
    // recv_timeout so we can also poll the child's liveness — if
    // the binary exits on its own (build failure, runtime panic),
    // we don't want to leave a zombie around the next time a save
    // fires.
    loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(events)) => {
                if !is_relevant(&events) {
                    continue;
                }
                eprintln!("\ncargo kick dev — change detected; restarting\n");
                kill_silently(&mut child);
                child = spawn_cargo_run(&root)?;
            }
            Ok(Err(errs)) => {
                eprintln!("cargo kick dev — watcher error: {errs:?}");
            }
            Err(RecvTimeoutError::Timeout) => {
                // Reap exited child without blocking — keeps zombies off
                // the table on platforms that don't auto-reap.
                let _ = child.try_wait();
            }
            Err(RecvTimeoutError::Disconnected) => {
                // The debouncer's sender hung up — unexpected; treat
                // as a fatal condition and exit the loop.
                kill_silently(&mut child);
                return Ok(());
            }
        }
    }
}

/// Spawn `cargo run` rooted at `root`. stdio inherited so the user
/// sees output live.
fn spawn_cargo_run(root: &Path) -> Result<Child, DevError> {
    Command::new("cargo")
        .arg("run")
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(DevError::CargoSpawn)
}

/// Best-effort kill — ignores errors because the child may already
/// be dead (compile failure, panic). We just want it gone before we
/// respawn.
fn kill_silently(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Filter the debounced events down to "something we care about".
/// Notify will fire on `.git/`, `target/`, IDE swap files, etc. We
/// reject those before pulling the trigger on a rebuild — saves a
/// lot of spurious restarts.
pub(crate) fn is_relevant(events: &[notify_debouncer_mini::DebouncedEvent]) -> bool {
    events.iter().any(|e| is_relevant_path(&e.path))
}

/// Heuristic: a path is relevant if it's a Rust source / TOML /
/// template-ish file *and* not inside an obvious noise directory
/// (target, .git, node_modules, build-script output dirs).
pub(crate) fn is_relevant_path(p: &Path) -> bool {
    // Reject noise directories anywhere in the path.
    for comp in p.components() {
        match comp.as_os_str().to_str() {
            Some("target") | Some(".git") | Some("node_modules") => return false,
            // Editor temp files commonly start with `.` or `~`.
            Some(s) if s.starts_with('~') => return false,
            Some(s) if s.ends_with("~") => return false,
            _ => {}
        }
    }
    // Accept rust + cargo + html/css/js/etc. Anything inside src/ is
    // a strong signal too — keep it permissive there.
    let in_src = p
        .components()
        .any(|c| c.as_os_str().to_str() == Some("src"));
    if in_src {
        return true;
    }
    matches!(
        p.extension().and_then(|s| s.to_str()),
        Some("rs" | "toml" | "lock" | "html" | "css" | "js" | "json" | "yaml" | "yml")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn is_relevant_path_accepts_rs_in_src() {
        assert!(is_relevant_path(&PathBuf::from("src/main.rs")));
        assert!(is_relevant_path(&PathBuf::from(
            "src/modules/posts/handlers.rs"
        )));
    }

    #[test]
    fn is_relevant_path_accepts_toml() {
        assert!(is_relevant_path(&PathBuf::from("Cargo.toml")));
    }

    #[test]
    fn is_relevant_path_rejects_target() {
        assert!(!is_relevant_path(&PathBuf::from(
            "target/debug/build/foo.rs"
        )));
        assert!(!is_relevant_path(&PathBuf::from(
            "/abs/proj/target/debug/app.exe"
        )));
    }

    #[test]
    fn is_relevant_path_rejects_git_and_node_modules() {
        assert!(!is_relevant_path(&PathBuf::from(".git/HEAD")));
        assert!(!is_relevant_path(&PathBuf::from(
            "node_modules/foo/index.js"
        )));
    }

    #[test]
    fn is_relevant_path_rejects_editor_temp_files() {
        assert!(!is_relevant_path(&PathBuf::from("src/main.rs~")));
        assert!(!is_relevant_path(&PathBuf::from("~scratch.rs")));
    }

    #[test]
    fn is_relevant_path_rejects_unrelated_extensions() {
        assert!(!is_relevant_path(&PathBuf::from("notes.txt")));
        assert!(!is_relevant_path(&PathBuf::from("logo.png")));
    }
}
