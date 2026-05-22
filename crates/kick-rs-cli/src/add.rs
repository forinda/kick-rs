//! `cargo kick add <feature>` — toggle an opt-in `kick-rs` feature
//! in the project's Cargo.toml.
//!
//! Why not just `cargo add kick-rs --features X`? Two reasons:
//!
//! 1. We validate the feature name against a known list, so a typo
//!    fails fast with a list of what's available instead of producing
//!    a working-but-useless `["typo"]` features array.
//! 2. We can describe each feature in one line — `cargo kick add list`
//!    is a quick reference without leaving the shell.
//!
//! The actual Cargo.toml mutation is done by `toml_edit` so the rest
//! of the file (layout, comments, dep ordering) is left alone.

use crate::generate::{find_project_root, GenerateError};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{value, Array, DocumentMut, Item, Value};

/// Decoded form of the `add` subcommand.
pub struct AddArgs {
    /// Feature name (e.g. `openapi`, `devtools`, `macros`, `config`).
    pub feature: String,
    /// Override the project root.
    pub project_root: Option<PathBuf>,
    /// Project's package name in Cargo.toml is `kick-rs`. Override if
    /// the adopter renamed it (unlikely but supported).
    pub dep_name: String,
}

/// Result of one `add` invocation.
#[derive(Debug, PartialEq, Eq)]
pub enum AddOutcome {
    /// Feature added to the `features` array (and the `features` key
    /// was created if it didn't exist).
    Added,
    /// Feature was already in the array.
    AlreadyEnabled,
}

#[derive(Debug)]
pub enum AddError {
    UnknownFeature {
        requested: String,
        known: &'static [&'static str],
    },
    DependencyNotFound(String),
    UnsupportedDependencyShape(String),
    // toml_edit::TomlError is ~128 bytes — box it to keep Result<_, AddError>
    // small enough that clippy::result_large_err is satisfied.
    Toml {
        path: PathBuf,
        source: Box<toml_edit::TomlError>,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    ProjectRoot(GenerateError),
}

impl std::fmt::Display for AddError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownFeature { requested, known } => {
                write!(f, "`{requested}` is not a known kick-rs feature. ")?;
                write!(f, "Known features: {}", known.join(", "))
            }
            Self::DependencyNotFound(dep) => write!(
                f,
                "couldn't find a `{dep}` entry under [dependencies] in this project's Cargo.toml."
            ),
            Self::UnsupportedDependencyShape(dep) => write!(
                f,
                "`{dep}` dep is in a shape this command can't safely mutate (likely a non-inline table). \
                 Edit it by hand and add the feature to the `features` array."
            ),
            Self::Toml { path, source } => {
                write!(f, "could not parse `{}`: {source}", path.display())
            }
            Self::Io { path, source } => write!(f, "I/O error at `{}`: {source}", path.display()),
            Self::ProjectRoot(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AddError {}

/// The features `cargo kick add` knows about, with a one-line
/// description for `cargo kick add list`. Keep in sync with the
/// `[features]` blocks in `crates/kick-rs/Cargo.toml`.
pub const KNOWN_FEATURES: &[(&str, &str)] = &[
    (
        "macros",
        "`#[service]`, `#[contributor]`, `#[get]`/`#[post]`/...",
    ),
    ("config", "Layered env / dotenv / TOML / JSON config loader"),
    ("openapi", "OpenApiPlugin + paths!() — serve /openapi.json"),
    (
        "devtools",
        "/__debug introspection endpoint (also needs .with_devtools())",
    ),
];

fn known_feature_names() -> &'static [&'static str] {
    // `const` slice can't be derived from KNOWN_FEATURES without
    // const-fn gymnastics, so we keep a parallel list. Kept tiny on
    // purpose — adding a feature here is a deliberate decision.
    &["macros", "config", "openapi", "devtools"]
}

/// Run the `add` flow.
pub fn add_feature(args: &AddArgs) -> Result<AddOutcome, AddError> {
    if !known_feature_names().contains(&args.feature.as_str()) {
        return Err(AddError::UnknownFeature {
            requested: args.feature.clone(),
            known: known_feature_names(),
        });
    }

    let root = match &args.project_root {
        Some(p) => p.clone(),
        None => find_project_root(Path::new(".")).map_err(AddError::ProjectRoot)?,
    };
    let cargo_toml = root.join("Cargo.toml");
    let contents = fs::read_to_string(&cargo_toml).map_err(|e| AddError::Io {
        path: cargo_toml.clone(),
        source: e,
    })?;
    let mut doc: DocumentMut = contents.parse().map_err(|e| AddError::Toml {
        path: cargo_toml.clone(),
        source: Box::new(e),
    })?;

    let outcome = mutate_features_array(&mut doc, &args.dep_name, &args.feature)?;

    fs::write(&cargo_toml, doc.to_string()).map_err(|e| AddError::Io {
        path: cargo_toml,
        source: e,
    })?;

    Ok(outcome)
}

/// Format-preserving mutation: find `[dependencies] <dep_name>` and
/// ensure `feature` is in its `features` array. Idempotent.
fn mutate_features_array(
    doc: &mut DocumentMut,
    dep_name: &str,
    feature: &str,
) -> Result<AddOutcome, AddError> {
    let deps = doc
        .get_mut("dependencies")
        .and_then(|i| i.as_table_like_mut())
        .ok_or_else(|| AddError::DependencyNotFound(dep_name.to_owned()))?;

    let dep_item = deps
        .get_mut(dep_name)
        .ok_or_else(|| AddError::DependencyNotFound(dep_name.to_owned()))?;

    upgrade_to_inline_table_if_needed(dep_item, dep_name)?;

    // After upgrade, `dep_item` is guaranteed to hold an inline table
    // (or already was one).
    let inline = dep_item
        .as_inline_table_mut()
        .ok_or_else(|| AddError::UnsupportedDependencyShape(dep_name.to_owned()))?;

    // Read or create the `features` array.
    let features_entry = inline.entry("features").or_insert_with(|| {
        let arr = Array::new();
        Value::Array(arr)
    });
    let arr = features_entry
        .as_array_mut()
        .ok_or_else(|| AddError::UnsupportedDependencyShape(dep_name.to_owned()))?;

    // Idempotent — bail without mutating if already present.
    let already = arr.iter().any(|v| v.as_str() == Some(feature));
    if already {
        return Ok(AddOutcome::AlreadyEnabled);
    }

    arr.push(feature);
    Ok(AddOutcome::Added)
}

/// `kick-rs = "0.1.0-alpha.1"` (string form) doesn't have a `features`
/// key to attach to. Promote it to an inline table form `{ version = ... }`
/// in place so the caller can edit `features` uniformly.
fn upgrade_to_inline_table_if_needed(item: &mut Item, dep_name: &str) -> Result<(), AddError> {
    let Item::Value(v) = item else {
        // Non-inline table (e.g. `[dependencies.kick-rs]` block) — we
        // don't try to convert that. Adopters using that shape can
        // edit features by hand, or convert to inline themselves.
        return Err(AddError::UnsupportedDependencyShape(dep_name.to_owned()));
    };
    match v {
        Value::String(s) => {
            let version = s.value().to_owned();
            let mut table = toml_edit::InlineTable::new();
            table.insert("version", value(version).into_value().unwrap());
            *v = Value::InlineTable(table);
            Ok(())
        }
        Value::InlineTable(_) => Ok(()),
        _ => Err(AddError::UnsupportedDependencyShape(dep_name.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on_str(input: &str, dep: &str, feature: &str) -> Result<(String, AddOutcome), AddError> {
        let mut doc: DocumentMut = input.parse().map_err(|e| AddError::Toml {
            path: PathBuf::from("<test>"),
            source: Box::new(e),
        })?;
        let outcome = mutate_features_array(&mut doc, dep, feature)?;
        Ok((doc.to_string(), outcome))
    }

    #[test]
    fn adds_feature_to_existing_features_array() {
        let input = r#"
[dependencies]
kick-rs = { version = "0.1", features = ["macros"] }
serde = "1"
"#;
        let (out, outcome) = run_on_str(input, "kick-rs", "openapi").unwrap();
        assert_eq!(outcome, AddOutcome::Added);
        assert!(
            out.contains(r#"features = ["macros", "openapi"]"#),
            "got:\n{out}"
        );
        // Other deps left alone.
        assert!(out.contains(r#"serde = "1""#));
    }

    #[test]
    fn idempotent_when_feature_already_present() {
        let input = r#"
[dependencies]
kick-rs = { version = "0.1", features = ["openapi"] }
"#;
        let (out, outcome) = run_on_str(input, "kick-rs", "openapi").unwrap();
        assert_eq!(outcome, AddOutcome::AlreadyEnabled);
        // Single occurrence — no double-add.
        assert_eq!(out.matches("openapi").count(), 1, "got:\n{out}");
    }

    #[test]
    fn promotes_string_dep_to_inline_table() {
        let input = r#"
[dependencies]
kick-rs = "0.1.0-alpha.1"
"#;
        let (out, outcome) = run_on_str(input, "kick-rs", "openapi").unwrap();
        assert_eq!(outcome, AddOutcome::Added);
        // Now in inline-table form with both version and features.
        assert!(out.contains("version = \"0.1.0-alpha.1\""), "got:\n{out}");
        assert!(out.contains(r#"features = ["openapi"]"#), "got:\n{out}");
    }

    #[test]
    fn creates_features_array_when_absent_on_inline_table() {
        let input = r#"
[dependencies]
kick-rs = { version = "0.1", path = "../kick-rs" }
"#;
        let (out, outcome) = run_on_str(input, "kick-rs", "macros").unwrap();
        assert_eq!(outcome, AddOutcome::Added);
        assert!(out.contains(r#"features = ["macros"]"#), "got:\n{out}");
        // Existing keys preserved.
        assert!(out.contains(r#"path = "../kick-rs""#));
    }

    #[test]
    fn errors_when_dep_not_found() {
        let input = "[dependencies]\nserde = \"1\"\n";
        let err = run_on_str(input, "kick-rs", "openapi").unwrap_err();
        assert!(
            matches!(err, AddError::DependencyNotFound(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn errors_on_non_inline_table_dep_shape() {
        // `[dependencies.kick-rs]` block — we refuse to convert these,
        // since collapsing them into inline form would be a noticeable
        // formatting change.
        let input = r#"
[dependencies.kick-rs]
version = "0.1"
features = ["macros"]
"#;
        let err = run_on_str(input, "kick-rs", "openapi").unwrap_err();
        assert!(
            matches!(err, AddError::UnsupportedDependencyShape(_)),
            "got {err:?}"
        );
    }

    // ─────────────────────── add_feature() — fs path ───────────────────────

    fn make_skeleton_with_cargo(dir: &Path, cargo: &str) {
        fs::create_dir_all(dir.join("src/modules")).unwrap();
        fs::write(dir.join("src/modules/mod.rs"), "pub mod hello;\n").unwrap();
        fs::write(dir.join("Cargo.toml"), cargo).unwrap();
    }

    #[test]
    fn add_feature_writes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_cargo(
            &root,
            r#"[package]
name = "x"
version = "0.1.0"
edition = "2021"

[dependencies]
kick-rs = { version = "0.1.0-alpha.1", features = ["macros"] }
"#,
        );

        let outcome = add_feature(&AddArgs {
            feature: "openapi".into(),
            project_root: Some(root.clone()),
            dep_name: "kick-rs".into(),
        })
        .unwrap();
        assert_eq!(outcome, AddOutcome::Added);

        let after = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(after.contains(r#"features = ["macros", "openapi"]"#));
    }

    #[test]
    fn add_feature_rejects_unknown_name() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_cargo(
            &root,
            "[package]\nname = \"x\"\n[dependencies]\nkick-rs = \"0.1\"\n",
        );

        let err = add_feature(&AddArgs {
            feature: "tofu".into(),
            project_root: Some(root.clone()),
            dep_name: "kick-rs".into(),
        })
        .unwrap_err();
        assert!(
            matches!(err, AddError::UnknownFeature { .. }),
            "got {err:?}"
        );
        // Cargo.toml left untouched.
        let after = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(!after.contains("tofu"));
    }
}
