//! `cargo kick g` — codegen into an existing kick-rs project.
//!
//! Currently ships one generator:
//!
//! - `g module <name>` — emit `src/modules/<name>/{mod.rs,handlers.rs}`
//!   and append `pub mod <name>;` to `src/modules/mod.rs`.
//!
//! Project root is auto-detected by walking up from `cwd` until we
//! find a directory containing `src/modules/mod.rs`. That single
//! anchor is what makes us "in a kick-rs project" for the purposes of
//! this command.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Decoded form of the `g module` subcommand.
pub struct GenerateModuleArgs {
    /// Module name (e.g. `posts`). Must be a valid Rust identifier:
    /// lowercase ASCII letters / digits / `_`, starting with a letter.
    /// Hyphens are rejected — Rust modules use underscores.
    pub name: String,
    /// Override the project root. Defaults to walking up from `cwd`.
    pub project_root: Option<PathBuf>,
    /// Allow overwriting `mod.rs` / `handlers.rs` if the module
    /// directory already exists.
    pub force: bool,
}

#[derive(Debug)]
pub enum GenerateError {
    InvalidName(String),
    ProjectRootNotFound(PathBuf),
    ModuleExists(PathBuf),
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(n) => write!(
                f,
                "`{n}` is not a valid module name. Use lowercase letters, digits, and underscores only (and start with a letter)."
            ),
            Self::ProjectRootNotFound(start) => write!(
                f,
                "could not find a kick-rs project from `{}` or any parent. Looking for `src/modules/mod.rs`.",
                start.display()
            ),
            Self::ModuleExists(p) => write!(
                f,
                "module directory `{}` already exists. Re-run with --force to overwrite the files inside.",
                p.display()
            ),
            Self::Io { path, source } => write!(f, "I/O error at `{}`: {source}", path.display()),
        }
    }
}

impl std::error::Error for GenerateError {}

/// Module-name validation. Snake-case only — hyphens disallowed
/// because Rust modules can't have them.
pub fn validate_module_name(name: &str) -> Result<(), GenerateError> {
    let bad = |reason: &str| -> GenerateError {
        GenerateError::InvalidName(format!("{name} ({reason})"))
    };
    if name.is_empty() {
        return Err(bad("empty"));
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() {
        return Err(bad("must start with a lowercase letter"));
    }
    for c in chars {
        let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_';
        if !ok {
            return Err(bad(
                "illegal character (hyphens not allowed in module names)",
            ));
        }
    }
    // A handful of keywords would shadow the language at the `pub mod
    // <name>;` site. Reject the most likely collisions.
    const RUST_KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
        "where", "while", "async", "await", "dyn",
    ];
    if RUST_KEYWORDS.contains(&name) {
        return Err(bad("is a Rust keyword"));
    }
    Ok(())
}

/// Walk up from `start` until we find a directory containing
/// `src/modules/mod.rs`. That's our project root.
pub fn find_project_root(start: &Path) -> Result<PathBuf, GenerateError> {
    let mut cur = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| GenerateError::Io {
                path: start.to_path_buf(),
                source: e,
            })?
            .join(start)
    };
    loop {
        if cur.join("src/modules/mod.rs").is_file() {
            return Ok(cur);
        }
        if !cur.pop() {
            return Err(GenerateError::ProjectRootNotFound(start.to_path_buf()));
        }
    }
}

/// Module skeleton: 2 files (mod.rs + handlers.rs).
const MOD_TMPL: &str = include_str!("../templates/generate/module/mod.rs.tmpl");
const HANDLERS_TMPL: &str = include_str!("../templates/generate/module/handlers.rs.tmpl");

fn render(template: &str, name: &str) -> String {
    template
        // module_name_snake is the same as module_name today (hyphens
        // are rejected), but keep the placeholder so the templates
        // are self-documenting about the intent.
        .replace("{{module_name_snake}}", name)
        .replace("{{module_name}}", name)
}

/// Run the `g module <name>` flow.
pub fn generate_module(args: &GenerateModuleArgs) -> Result<PathBuf, GenerateError> {
    validate_module_name(&args.name)?;

    let root = match &args.project_root {
        Some(p) => p.clone(),
        None => find_project_root(Path::new("."))?,
    };
    if !root.join("src/modules/mod.rs").is_file() {
        return Err(GenerateError::ProjectRootNotFound(root));
    }

    let module_dir = root.join("src/modules").join(&args.name);
    if module_dir.exists() && !args.force {
        return Err(GenerateError::ModuleExists(module_dir));
    }
    fs::create_dir_all(&module_dir).map_err(|e| GenerateError::Io {
        path: module_dir.clone(),
        source: e,
    })?;

    let mod_rs = module_dir.join("mod.rs");
    fs::write(&mod_rs, render(MOD_TMPL, &args.name)).map_err(|e| GenerateError::Io {
        path: mod_rs.clone(),
        source: e,
    })?;

    let handlers_rs = module_dir.join("handlers.rs");
    fs::write(&handlers_rs, render(HANDLERS_TMPL, &args.name)).map_err(|e| GenerateError::Io {
        path: handlers_rs.clone(),
        source: e,
    })?;

    // Idempotently append `pub mod <name>;` to src/modules/mod.rs.
    let modules_mod = root.join("src/modules/mod.rs");
    let mut contents = fs::read_to_string(&modules_mod).map_err(|e| GenerateError::Io {
        path: modules_mod.clone(),
        source: e,
    })?;
    let decl = format!("pub mod {};", args.name);
    if !contents.lines().any(|line| line.trim() == decl) {
        // Make sure we land on a newline before appending so we don't
        // glue onto a no-trailing-newline file.
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(&decl);
        contents.push('\n');
        fs::write(&modules_mod, contents).map_err(|e| GenerateError::Io {
            path: modules_mod.clone(),
            source: e,
        })?;
    }

    Ok(module_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal "project" inside `dir` so `find_project_root`
    /// and the codegen can operate on it.
    fn make_skeleton(dir: &Path) {
        fs::create_dir_all(dir.join("src/modules")).unwrap();
        fs::write(dir.join("src/modules/mod.rs"), "pub mod hello;\n").unwrap();
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    }

    #[test]
    fn validate_module_name_accepts_typical_names() {
        assert!(validate_module_name("posts").is_ok());
        assert!(validate_module_name("user_session").is_ok());
        assert!(validate_module_name("v1").is_ok());
    }

    #[test]
    fn validate_module_name_rejects_bad_names() {
        assert!(validate_module_name("").is_err());
        assert!(validate_module_name("Posts").is_err()); // upper
        assert!(validate_module_name("has-hyphen").is_err()); // hyphen
        assert!(validate_module_name("1leading").is_err()); // leading digit
        assert!(validate_module_name("fn").is_err()); // keyword
        assert!(validate_module_name("type").is_err()); // keyword
    }

    #[test]
    fn find_project_root_walks_up_until_modules_anchor() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton(&root);

        // Find from project root itself
        assert_eq!(find_project_root(&root).unwrap(), root);

        // Find from nested directory inside the project
        let deep = root.join("src/modules/hello");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(find_project_root(&deep).unwrap(), root);
    }

    #[test]
    fn find_project_root_errors_outside_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        // tempdir is empty — no modules anchor anywhere
        let err = find_project_root(tmp.path()).unwrap_err();
        assert!(matches!(err, GenerateError::ProjectRootNotFound(_)));
    }

    #[test]
    fn generate_module_writes_files_and_appends_modules_mod() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton(&root);

        let args = GenerateModuleArgs {
            name: "posts".into(),
            project_root: Some(root.clone()),
            force: false,
        };
        let module_dir = generate_module(&args).unwrap();
        assert_eq!(module_dir, root.join("src/modules/posts"));

        let mod_rs = fs::read_to_string(module_dir.join("mod.rs")).unwrap();
        assert!(mod_rs.contains(r#"define_module("posts")"#));
        assert!(mod_rs.contains(r#".prefix("/posts")"#));

        let handlers_rs = fs::read_to_string(module_dir.join("handlers.rs")).unwrap();
        assert!(handlers_rs.contains("posts index"));

        // `pub mod posts;` got appended exactly once.
        let modules_mod = fs::read_to_string(root.join("src/modules/mod.rs")).unwrap();
        let count = modules_mod.matches("pub mod posts;").count();
        assert_eq!(count, 1, "expected one append, got {count}: {modules_mod}");
    }

    #[test]
    fn generate_module_appending_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton(&root);

        let args = GenerateModuleArgs {
            name: "posts".into(),
            project_root: Some(root.clone()),
            force: false,
        };
        generate_module(&args).unwrap();

        // Second run with --force shouldn't double-up the re-export.
        let args2 = GenerateModuleArgs {
            force: true,
            ..args
        };
        generate_module(&args2).unwrap();

        let modules_mod = fs::read_to_string(root.join("src/modules/mod.rs")).unwrap();
        assert_eq!(
            modules_mod.matches("pub mod posts;").count(),
            1,
            "double-append on second generate: {modules_mod}"
        );
    }

    #[test]
    fn generate_module_refuses_existing_dir_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton(&root);
        fs::create_dir_all(root.join("src/modules/posts")).unwrap();

        let err = generate_module(&GenerateModuleArgs {
            name: "posts".into(),
            project_root: Some(root.clone()),
            force: false,
        })
        .unwrap_err();
        assert!(matches!(err, GenerateError::ModuleExists(_)), "got {err:?}");
    }
}
