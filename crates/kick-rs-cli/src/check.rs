//! `cargo kick check` — lint an existing kick-rs project for common
//! misconfigurations the compiler doesn't catch.
//!
//! Pure-text scanners, no `syn` round-trip. The patterns we recognize
//! are the same shapes the scaffold + `cargo kick g` produce:
//!
//! - `pub mod X;` in `src/modules/mod.rs`
//! - `.module(modules::X::define())` in `src/main.rs`
//! - `pub mod Y;` in `src/modules/<X>/mod.rs`
//! - `.service::<Pascal>()` / `.contribute(Pascal)` in
//!   `src/modules/<X>/mod.rs`
//! - `#[service]` / `#[contributor]` annotations in the corresponding
//!   `src/modules/<X>/<Y>.rs`
//!
//! Findings are reported with the file path + a one-line hint. The
//! CLI exits non-zero when any finding is non-empty; that makes
//! `cargo kick check` a useful CI gate after generators have run.

use crate::generate::{find_project_root, to_pascal_case, GenerateError};
use std::fs;
use std::path::{Path, PathBuf};

/// Decoded form of the `check` subcommand.
pub struct CheckArgs {
    /// Override the project root.
    pub project_root: Option<PathBuf>,
}

/// One lint finding. The `code` is a stable identifier — adopters
/// can pin a CI check against a specific lint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Stable lint code (e.g. `RK_K_UNMOUNTED_MODULE`).
    pub code: &'static str,
    /// One-line human-readable summary.
    pub message: String,
    /// File path the finding is anchored to.
    pub path: PathBuf,
}

/// Per-run summary returned by [`run`].
#[derive(Debug, Default)]
pub struct CheckReport {
    pub findings: Vec<Finding>,
}

impl CheckReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

#[derive(Debug)]
pub enum CheckError {
    ProjectRoot(GenerateError),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProjectRoot(e) => write!(f, "{e}"),
            Self::Io { path, source } => write!(f, "I/O error at `{}`: {source}", path.display()),
        }
    }
}

impl std::error::Error for CheckError {}

/// Walk the project and produce a [`CheckReport`].
pub fn run(args: &CheckArgs) -> Result<CheckReport, CheckError> {
    let root = match &args.project_root {
        Some(p) => p.clone(),
        None => find_project_root(Path::new(".")).map_err(CheckError::ProjectRoot)?,
    };

    let mut report = CheckReport::default();

    // ── M1: modules declared in src/modules/mod.rs but never mounted
    //        in src/main.rs (or no main.rs found).
    let modules_mod_rs = root.join("src/modules/mod.rs");
    let modules_mod_body = read_to_string_optional(&modules_mod_rs)?;
    let declared = pub_mod_names(&modules_mod_body);
    let main_rs = root.join("src/main.rs");
    let main_body = read_to_string_optional(&main_rs)?;

    for name in &declared {
        // `.module(modules::<name>::define())` is the only place this
        // appears in a normal main.rs. Match liberally on the
        // substring to tolerate spacing / line breaks.
        let needle = format!("modules::{name}::define()");
        if !main_body.contains(&needle) && !main_body.is_empty() {
            report.findings.push(Finding {
                code: "RK_K_UNMOUNTED_MODULE",
                message: format!(
                    "module `{name}` is declared in src/modules/mod.rs but not mounted in src/main.rs (expected `.module({needle})`)"
                ),
                path: main_rs.clone(),
            });
        }
    }

    // ── R1: pub mod X; line whose <X>/ directory (or <X>.rs file)
    //        doesn't exist on disk. Catches a manual delete that
    //        left the re-export behind.
    for name in &declared {
        let as_dir = root.join("src/modules").join(name);
        let as_file = root.join("src/modules").join(format!("{name}.rs"));
        if !as_dir.is_dir() && !as_file.is_file() {
            report.findings.push(Finding {
                code: "RK_K_STALE_PUB_MOD",
                message: format!(
                    "src/modules/mod.rs declares `pub mod {name};` but neither src/modules/{name}/ nor src/modules/{name}.rs exists"
                ),
                path: modules_mod_rs.clone(),
            });
        }
    }

    // ── S1: services / contributors declared in `<module>/<file>.rs`
    //        (via `#[service]` / `#[contributor]`) but not registered
    //        in `<module>/mod.rs`.
    for name in &declared {
        let module_dir = root.join("src/modules").join(name);
        if !module_dir.is_dir() {
            continue;
        }
        let module_mod_rs = module_dir.join("mod.rs");
        let module_body = read_to_string_optional(&module_mod_rs)?;

        let decls = collect_decls(&module_dir)?;
        for d in decls {
            let registered = match d.kind {
                DeclKind::Service => module_body.contains(&format!(".service::<{}>()", d.pascal)),
                DeclKind::Contributor => {
                    module_body.contains(&format!(".contribute({})", d.pascal))
                }
            };
            if !registered {
                let (lint_code, builder_method) = match d.kind {
                    DeclKind::Service => (
                        "RK_K_UNREGISTERED_SERVICE",
                        format!(".service::<{}>()", d.pascal),
                    ),
                    DeclKind::Contributor => (
                        "RK_K_UNREGISTERED_CONTRIBUTOR",
                        format!(".contribute({})", d.pascal),
                    ),
                };
                report.findings.push(Finding {
                    code: lint_code,
                    message: format!(
                        "{} `{}` declared in src/modules/{}/{}.rs but not registered (expected `{}`)",
                        match d.kind {
                            DeclKind::Service => "service",
                            DeclKind::Contributor => "contributor",
                        },
                        d.pascal,
                        name,
                        d.file_stem,
                        builder_method,
                    ),
                    path: module_mod_rs.clone(),
                });
            }
        }
    }

    Ok(report)
}

/// Helper: read a file; treat missing files as empty so the
/// individual lints can choose how to interpret absence (M1 skips,
/// S1 just doesn't fire because there's nothing inside).
fn read_to_string_optional(path: &Path) -> Result<String, CheckError> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(CheckError::Io {
            path: path.to_path_buf(),
            source: e,
        }),
    }
}

/// Extract every `pub mod <name>;` declared at the top level of the
/// given source. Whitespace between `pub`, `mod`, name, and `;` is
/// tolerated; multi-attribute lines aren't (rare in generated code).
pub(crate) fn pub_mod_names(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        // Strip leading `pub mod ` and trailing `;`.
        let Some(rest) = trimmed.strip_prefix("pub mod ") else {
            continue;
        };
        let Some(name) = rest.strip_suffix(';') else {
            continue;
        };
        let name = name.trim();
        // Reject anything that isn't a plain identifier — `pub mod x::y;`
        // is illegal anyway and an `as` rename is rare here.
        if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty() {
            out.push(name.to_owned());
        }
    }
    out
}

/// What kind of registration is missing for a given file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclKind {
    Service,
    Contributor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Decl {
    kind: DeclKind,
    pascal: String,
    /// File stem (e.g. `email_sender` for `email_sender.rs`).
    file_stem: String,
}

/// Scan `module_dir` for .rs files containing `#[service]` or
/// `#[contributor]` and extract the corresponding type / fn name.
fn collect_decls(module_dir: &Path) -> Result<Vec<Decl>, CheckError> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(module_dir) {
        Ok(e) => e,
        Err(_) => return Ok(out),
    };
    for entry in entries.filter_map(Result::ok) {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if p.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let stem = match p.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };
        // `mod.rs` and `handlers.rs` are never decls of services or
        // contributors — skip them up front to keep scans cheap.
        if stem == "mod" || stem == "handlers" {
            continue;
        }
        let body = match fs::read_to_string(&p) {
            Ok(b) => b,
            Err(_) => continue,
        };
        // The macro patterns. We just check for the bare attribute —
        // the file stem gives us the registration name via
        // PascalCase, matching how `cargo kick g service|contributor`
        // emits them.
        if body.contains("#[service]") {
            out.push(Decl {
                kind: DeclKind::Service,
                pascal: to_pascal_case(stem),
                file_stem: stem.to_owned(),
            });
        }
        if body.contains("#[contributor]") {
            out.push(Decl {
                kind: DeclKind::Contributor,
                pascal: to_pascal_case(stem),
                file_stem: stem.to_owned(),
            });
        }
    }
    Ok(out)
}

/// Render the report for the CLI.
pub fn render(report: &CheckReport) -> String {
    if report.findings.is_empty() {
        return "kick-rs check: ✓ clean\n".to_owned();
    }
    let mut out = format!("kick-rs check: {} finding(s)\n\n", report.findings.len());
    for f in &report.findings {
        out.push_str(&format!("  [{}] {}\n", f.code, f.message));
        out.push_str(&format!("      → {}\n", f.path.display()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pub_mod_names_extracts_identifiers() {
        let body = "pub mod handlers;\npub mod email_sender;\nuse foo;\n";
        assert_eq!(pub_mod_names(body), vec!["handlers", "email_sender"]);
    }

    #[test]
    fn pub_mod_names_handles_whitespace() {
        // Leading whitespace on the line is trimmed before matching.
        // Extra space between `mod` and the name happens to also work
        // (we `.trim()` the captured name) — covered here to lock in
        // the actual behavior.
        let body = "    pub mod posts;\n  pub mod  users;\n";
        assert_eq!(pub_mod_names(body), vec!["posts", "users"]);
    }

    #[test]
    fn pub_mod_names_skips_comments_and_other_lines() {
        let body = "// pub mod commented_out;\npub mod real;\n";
        assert_eq!(pub_mod_names(body), vec!["real"]);
    }

    /// Build a minimal project skeleton for the integration tests.
    fn make_project(dir: &Path) {
        fs::create_dir_all(dir.join("src/modules/hello")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.0.1\"\n",
        )
        .unwrap();
        fs::write(dir.join("src/modules/mod.rs"), "pub mod hello;\n").unwrap();
        fs::write(
            dir.join("src/modules/hello/mod.rs"),
            "pub mod handlers;\n\
             use kick_rs::{define_module, Module};\n\
             pub fn define() -> Module {\n    \
                 define_module(\"hello\").prefix(\"/hello\").build()\n\
             }\n",
        )
        .unwrap();
        fs::write(dir.join("src/modules/hello/handlers.rs"), "// stub\n").unwrap();
        fs::write(
            dir.join("src/main.rs"),
            "use kick_rs::bootstrap;\n\
             mod modules;\n\
             #[tokio::main]\n\
             async fn main() {\n    \
                 bootstrap().module(modules::hello::define()).listen(\"0\").await.unwrap();\n\
             }\n",
        )
        .unwrap();
    }

    #[test]
    fn clean_project_reports_no_findings() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_project(&root);

        let report = run(&CheckArgs {
            project_root: Some(root.clone()),
        })
        .unwrap();
        assert!(report.is_clean(), "got {:?}", report.findings);
    }

    #[test]
    fn unmounted_module_is_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_project(&root);
        // Append an unmounted module declaration; the directory + an
        // empty mod.rs are needed to keep R1 happy.
        let mut top = fs::read_to_string(root.join("src/modules/mod.rs")).unwrap();
        top.push_str("pub mod posts;\n");
        fs::write(root.join("src/modules/mod.rs"), top).unwrap();
        fs::create_dir_all(root.join("src/modules/posts")).unwrap();
        fs::write(root.join("src/modules/posts/mod.rs"), "").unwrap();

        let report = run(&CheckArgs {
            project_root: Some(root.clone()),
        })
        .unwrap();
        let codes: Vec<_> = report.findings.iter().map(|f| f.code).collect();
        assert!(codes.contains(&"RK_K_UNMOUNTED_MODULE"), "got {codes:?}");
    }

    #[test]
    fn stale_pub_mod_is_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_project(&root);
        // Declare a module whose directory + file don't exist.
        let mut top = fs::read_to_string(root.join("src/modules/mod.rs")).unwrap();
        top.push_str("pub mod ghosts;\n");
        fs::write(root.join("src/modules/mod.rs"), top).unwrap();

        let report = run(&CheckArgs {
            project_root: Some(root.clone()),
        })
        .unwrap();
        let codes: Vec<_> = report.findings.iter().map(|f| f.code).collect();
        assert!(codes.contains(&"RK_K_STALE_PUB_MOD"), "got {codes:?}");
    }

    #[test]
    fn unregistered_service_is_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_project(&root);
        // Drop an unregistered service file into the hello module.
        fs::write(
            root.join("src/modules/hello/email_sender.rs"),
            "use kick_rs::service;\n#[service]\npub struct EmailSender;\n",
        )
        .unwrap();
        // Add the pub mod entry so the module compiles in principle,
        // but DON'T add the .service::<EmailSender>() call to mod.rs.
        let mut mod_rs = fs::read_to_string(root.join("src/modules/hello/mod.rs")).unwrap();
        mod_rs.insert_str(0, "pub mod email_sender;\n");
        fs::write(root.join("src/modules/hello/mod.rs"), mod_rs).unwrap();

        let report = run(&CheckArgs {
            project_root: Some(root.clone()),
        })
        .unwrap();
        let codes: Vec<_> = report.findings.iter().map(|f| f.code).collect();
        assert!(
            codes.contains(&"RK_K_UNREGISTERED_SERVICE"),
            "got {codes:?}"
        );
    }

    #[test]
    fn unregistered_contributor_is_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_project(&root);
        fs::write(
            root.join("src/modules/hello/load_user.rs"),
            "use kick_rs::contributor;\n#[contributor]\npub async fn LoadUser() {}\n",
        )
        .unwrap();
        let mut mod_rs = fs::read_to_string(root.join("src/modules/hello/mod.rs")).unwrap();
        mod_rs.insert_str(0, "pub mod load_user;\n");
        fs::write(root.join("src/modules/hello/mod.rs"), mod_rs).unwrap();

        let report = run(&CheckArgs {
            project_root: Some(root.clone()),
        })
        .unwrap();
        let codes: Vec<_> = report.findings.iter().map(|f| f.code).collect();
        assert!(
            codes.contains(&"RK_K_UNREGISTERED_CONTRIBUTOR"),
            "got {codes:?}"
        );
    }

    #[test]
    fn registered_service_passes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_project(&root);
        fs::write(
            root.join("src/modules/hello/email_sender.rs"),
            "use kick_rs::service;\n#[service]\npub struct EmailSender;\n",
        )
        .unwrap();
        // mod.rs already declares define() — patch in the registration.
        fs::write(
            root.join("src/modules/hello/mod.rs"),
            "pub mod handlers;\n\
             pub mod email_sender;\n\
             use kick_rs::{define_module, Module};\n\
             use email_sender::EmailSender;\n\
             pub fn define() -> Module {\n    \
                 define_module(\"hello\").prefix(\"/hello\").service::<EmailSender>().build()\n\
             }\n",
        )
        .unwrap();

        let report = run(&CheckArgs {
            project_root: Some(root.clone()),
        })
        .unwrap();
        // The new service file declares pub mod email_sender, no R1 hit.
        // It IS registered via .service::<EmailSender>(), so no S1.
        // No other findings either — clean.
        assert!(report.is_clean(), "got {:?}", report.findings);
    }

    #[test]
    fn render_shows_findings_or_clean_marker() {
        let mut r = CheckReport::default();
        assert!(render(&r).contains("✓ clean"));
        r.findings.push(Finding {
            code: "RK_K_UNMOUNTED_MODULE",
            message: "x".into(),
            path: PathBuf::from("src/main.rs"),
        });
        let out = render(&r);
        assert!(out.contains("RK_K_UNMOUNTED_MODULE"));
        assert!(out.contains("src/main.rs"));
    }
}
