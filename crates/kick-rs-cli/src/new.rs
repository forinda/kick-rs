//! `cargo kick new` — scaffold a new kick-rs project.
//!
//! Walks the embedded template manifest and writes each file under a
//! freshly-created project directory, substituting `{{project_name}}`
//! / `{{project_name_snake}}` into the contents.

use crate::templates::{render, Vars, FILES};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Decoded form of the `new` subcommand's arguments.
pub struct NewArgs {
    pub name: String,
    pub path: Option<PathBuf>,
    pub force: bool,
}

/// User-facing error from the `new` flow. Stringly-typed because the
/// CLI shows them to a human, not a downstream caller.
#[derive(Debug)]
pub enum NewError {
    InvalidName(String),
    AlreadyExists(PathBuf),
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for NewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(name) => write!(
                f,
                "`{name}` is not a valid project name. Use lowercase letters, digits, hyphens, and underscores only (and start with a letter)."
            ),
            Self::AlreadyExists(p) => write!(
                f,
                "destination `{}` already exists. Re-run with --force to use it anyway (existing files inside are NOT removed).",
                p.display()
            ),
            Self::Io { path, source } => write!(f, "I/O error at `{}`: {source}", path.display()),
        }
    }
}

impl std::error::Error for NewError {}

/// Validate the project name. Mirrors what cargo itself accepts for
/// crate names: lowercase ASCII letters / digits / `-` / `_`, must
/// start with a letter, no consecutive hyphens.
pub fn validate_name(name: &str) -> Result<(), NewError> {
    let bad = |reason: &str| -> NewError { NewError::InvalidName(format!("{name} ({reason})")) };

    if name.is_empty() {
        return Err(bad("empty"));
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() {
        return Err(bad("must start with a lowercase letter"));
    }
    for c in chars {
        let ok = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_';
        if !ok {
            return Err(bad("illegal character"));
        }
    }
    Ok(())
}

/// Run the scaffold against the given args. Returns the path that was
/// written so the caller can echo it back.
pub fn run(args: &NewArgs) -> Result<PathBuf, NewError> {
    validate_name(&args.name)?;

    let dest = args
        .path
        .clone()
        .unwrap_or_else(|| PathBuf::from(&args.name));

    if dest.exists() && !args.force {
        return Err(NewError::AlreadyExists(dest));
    }
    if !dest.exists() {
        fs::create_dir_all(&dest).map_err(|e| NewError::Io {
            path: dest.clone(),
            source: e,
        })?;
    }

    let vars = Vars {
        project_name: &args.name,
        project_name_snake: &args.name.replace('-', "_"),
    };

    for (rel, contents) in FILES {
        write_one(&dest, rel, contents, &vars)?;
    }

    Ok(dest)
}

fn write_one(dest: &Path, rel: &str, template: &str, vars: &Vars<'_>) -> Result<(), NewError> {
    let target = dest.join(rel);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| NewError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let rendered = render(template, vars);
    fs::write(&target, rendered).map_err(|e| NewError::Io {
        path: target.clone(),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_name_accepts_typical_names() {
        assert!(validate_name("my-app").is_ok());
        assert!(validate_name("my_app").is_ok());
        assert!(validate_name("api1").is_ok());
        assert!(validate_name("a").is_ok());
    }

    #[test]
    fn validate_name_rejects_bad_names() {
        assert!(validate_name("").is_err());
        assert!(validate_name("1leading-digit").is_err());
        assert!(validate_name("UPPER").is_err());
        assert!(validate_name("has space").is_err());
        assert!(validate_name("has.dot").is_err());
    }

    #[test]
    fn run_scaffolds_into_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("my-app");
        let args = NewArgs {
            name: "my-app".into(),
            path: Some(target.clone()),
            force: false,
        };
        let written = run(&args).unwrap();
        assert_eq!(written, target);

        // A couple of representative files exist with substitution applied.
        let cargo = std::fs::read_to_string(target.join("Cargo.toml")).unwrap();
        assert!(cargo.contains(r#"name        = "my-app""#));
        let envex = std::fs::read_to_string(target.join(".env.example")).unwrap();
        assert!(
            envex.contains("my_app=debug"),
            "expected snake-cased target in .env.example, got: {envex}"
        );
        // Module tree got laid out.
        assert!(target.join("src/modules/hello/handlers.rs").is_file());
    }

    #[test]
    fn run_refuses_to_overwrite_existing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("existing");
        std::fs::create_dir(&target).unwrap();

        let args = NewArgs {
            name: "existing".into(),
            path: Some(target.clone()),
            force: false,
        };
        let err = run(&args).unwrap_err();
        assert!(matches!(err, NewError::AlreadyExists(_)), "got {err:?}");
    }

    #[test]
    fn run_with_force_writes_into_existing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("existing");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("untouched.txt"), "stay put").unwrap();

        let args = NewArgs {
            name: "existing".into(),
            path: Some(target.clone()),
            force: true,
        };
        run(&args).unwrap();

        // Pre-existing file is preserved (we don't wipe), template
        // files now exist alongside.
        assert!(target.join("untouched.txt").is_file());
        assert!(target.join("Cargo.toml").is_file());
    }
}
