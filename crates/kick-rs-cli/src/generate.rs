//! `cargo kick g` — codegen into an existing kick-rs project.
//!
//! Currently ships:
//!
//! - `g module <name>` — emit `src/modules/<name>/{mod.rs,handlers.rs}`
//!   and append `pub mod <name>;` to `src/modules/mod.rs`.
//! - `g service <module>/<name>` — emit
//!   `src/modules/<module>/<name>.rs` containing a `#[service]`-derived
//!   stub, and append `pub mod <name>;` to the parent module's `mod.rs`.
//! - `g contributor <module>/<name>` — emit
//!   `src/modules/<module>/<name>.rs` containing a `#[contributor]`
//!   async fn + Output struct, and append `pub mod <name>;` to the
//!   parent module's `mod.rs`.
//!
//! Project root is auto-detected by walking up from `cwd` until we
//! find a directory containing `src/modules/mod.rs`. That single
//! anchor is what makes us "in a kick-rs project" for the purposes of
//! this command.

use crate::register::{insert_chain_call_after_anchor, insert_use_after_last_use, RegisterOutcome};
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
    /// Try to auto-insert `.module(modules::<name>::define())` into
    /// `src/main.rs`. When the patterns aren't found, the caller falls
    /// back to printing a manual hint.
    pub auto_register: bool,
}

#[derive(Debug)]
pub enum GenerateError {
    InvalidName(String),
    InvalidSpec(String),
    ProjectRootNotFound(PathBuf),
    ModuleExists(PathBuf),
    ModuleMissing(PathBuf),
    FileExists(PathBuf),
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(n) => write!(
                f,
                "`{n}` is not a valid name. Use lowercase letters, digits, and underscores only (and start with a letter)."
            ),
            Self::InvalidSpec(s) => write!(
                f,
                "`{s}` is not a valid spec. Expected `<module>/<name>` (e.g. `users/email_sender`)."
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
            Self::ModuleMissing(p) => write!(
                f,
                "parent module `{}` does not exist. Generate it first with `cargo kick g module`.",
                p.display()
            ),
            Self::FileExists(p) => write!(
                f,
                "file `{}` already exists. Re-run with --force to overwrite.",
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

/// Service skeleton: 1 file.
const SERVICE_TMPL: &str = include_str!("../templates/generate/service/file.rs.tmpl");

/// Contributor skeleton: 1 file.
const CONTRIBUTOR_TMPL: &str = include_str!("../templates/generate/contributor/file.rs.tmpl");

/// Convert a snake_case identifier to PascalCase. Used to derive the
/// service struct name from its file name (`email_sender` → `EmailSender`).
///
/// Assumes the input has already been validated as a snake_case
/// identifier (no leading digits, no hyphens, etc).
pub fn to_pascal_case(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut upper_next = true;
    for c in snake.chars() {
        if c == '_' {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn render(template: &str, name: &str) -> String {
    template
        // module_name_snake is the same as module_name today (hyphens
        // are rejected), but keep the placeholder so the templates
        // are self-documenting about the intent.
        .replace("{{module_name_snake}}", name)
        .replace("{{module_name}}", name)
}

fn render_service(template: &str, snake: &str, pascal: &str) -> String {
    template
        .replace("{{service_snake}}", snake)
        .replace("{{service_pascal}}", pascal)
}

fn render_contributor(template: &str, snake: &str, pascal: &str) -> String {
    template
        .replace("{{contributor_snake}}", snake)
        .replace("{{contributor_pascal}}", pascal)
}

/// Result of `generate_module` — the directory written + how the
/// `main.rs` auto-register attempt fared.
#[derive(Debug)]
pub struct GenerateModuleResult {
    pub module_dir: PathBuf,
    pub register: RegisterOutcome,
}

/// Run the `g module <name>` flow.
pub fn generate_module(args: &GenerateModuleArgs) -> Result<GenerateModuleResult, GenerateError> {
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

    let modules_mod = root.join("src/modules/mod.rs");
    ensure_pub_mod_line(&modules_mod, &args.name)?;

    let register = if args.auto_register {
        try_register_module_in_main(&root, &args.name)?
    } else {
        RegisterOutcome::Skipped
    };

    Ok(GenerateModuleResult {
        module_dir,
        register,
    })
}

/// Try to insert `.module(modules::<name>::define())` into
/// `src/main.rs` next to existing `.module(...)` calls (or right after
/// `bootstrap()` if none).
fn try_register_module_in_main(root: &Path, name: &str) -> Result<RegisterOutcome, GenerateError> {
    let main_rs = root.join("src/main.rs");
    if !main_rs.is_file() {
        return Ok(RegisterOutcome::TargetMissing);
    }
    let mut contents = fs::read_to_string(&main_rs).map_err(|e| GenerateError::Io {
        path: main_rs.clone(),
        source: e,
    })?;

    let signature = format!("modules::{name}::define()");
    if contents.contains(&signature) {
        return Ok(RegisterOutcome::AlreadyRegistered);
    }

    let call = format!(".module(modules::{name}::define())");
    let inserted = insert_chain_call_after_anchor(&mut contents, ".module(modules::", &call)
        || insert_chain_call_after_anchor(&mut contents, "bootstrap()", &call);
    if !inserted {
        return Ok(RegisterOutcome::AnchorNotFound);
    }

    fs::write(&main_rs, contents).map_err(|e| GenerateError::Io {
        path: main_rs.clone(),
        source: e,
    })?;
    Ok(RegisterOutcome::Inserted)
}

/// Idempotently append `pub mod <name>;` to `target` if it isn't
/// already present (line-equality match, ignoring leading/trailing
/// whitespace).
fn ensure_pub_mod_line(target: &Path, name: &str) -> Result<(), GenerateError> {
    let mut contents = fs::read_to_string(target).map_err(|e| GenerateError::Io {
        path: target.to_path_buf(),
        source: e,
    })?;
    let decl = format!("pub mod {name};");
    if contents.lines().any(|line| line.trim() == decl) {
        return Ok(());
    }
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str(&decl);
    contents.push('\n');
    fs::write(target, contents).map_err(|e| GenerateError::Io {
        path: target.to_path_buf(),
        source: e,
    })
}

// ─────────────────────────── g service ───────────────────────────────

/// Decoded form of the `g service` subcommand.
pub struct GenerateServiceArgs {
    /// `<module>/<service_snake>` spec.
    pub spec: String,
    /// Override the project root.
    pub project_root: Option<PathBuf>,
    /// Overwrite the service file if it already exists.
    pub force: bool,
    /// Try to auto-insert `use <name>::<Pascal>;` + `.service::<Pascal>()`
    /// into the parent module's `mod.rs`.
    pub auto_register: bool,
}

/// Split `<module>/<name>` and validate each half. Shared by `g service`
/// and `g contributor` — both expect the same shape (a module name and
/// a snake_case item name inside it).
fn parse_kind_spec(spec: &str) -> Result<(&str, &str), GenerateError> {
    let (module, name) = spec
        .split_once('/')
        .ok_or_else(|| GenerateError::InvalidSpec(spec.to_owned()))?;
    if module.is_empty() || name.is_empty() {
        return Err(GenerateError::InvalidSpec(spec.to_owned()));
    }
    if name.contains('/') {
        return Err(GenerateError::InvalidSpec(spec.to_owned()));
    }
    validate_module_name(module)?;
    validate_module_name(name)?;
    Ok((module, name))
}

/// Result of `generate_service` — the file written + auto-register outcome.
#[derive(Debug)]
pub struct GenerateServiceResult {
    pub file: PathBuf,
    pub register: RegisterOutcome,
}

/// Run the `g service <module>/<service_snake>` flow.
pub fn generate_service(
    args: &GenerateServiceArgs,
) -> Result<GenerateServiceResult, GenerateError> {
    let (module, service_snake) = parse_kind_spec(&args.spec)?;
    let service_pascal = to_pascal_case(service_snake);

    let root = match &args.project_root {
        Some(p) => p.clone(),
        None => find_project_root(Path::new("."))?,
    };
    let module_mod_rs = root.join("src/modules").join(module).join("mod.rs");
    if !module_mod_rs.is_file() {
        return Err(GenerateError::ModuleMissing(
            root.join("src/modules").join(module),
        ));
    }

    let service_file = root
        .join("src/modules")
        .join(module)
        .join(format!("{service_snake}.rs"));
    if service_file.exists() && !args.force {
        return Err(GenerateError::FileExists(service_file));
    }

    let rendered = render_service(SERVICE_TMPL, service_snake, &service_pascal);
    fs::write(&service_file, rendered).map_err(|e| GenerateError::Io {
        path: service_file.clone(),
        source: e,
    })?;

    ensure_pub_mod_line(&module_mod_rs, service_snake)?;

    let register = if args.auto_register {
        try_register_in_define_builder(
            &module_mod_rs,
            service_snake,
            &service_pascal,
            DefineBuilderKind::Service,
        )?
    } else {
        RegisterOutcome::Skipped
    };

    Ok(GenerateServiceResult {
        file: service_file,
        register,
    })
}

/// Which builder method to insert. The format of the call differs
/// between services (`.service::<X>()`) and contributors (`.contribute(X)`).
#[derive(Clone, Copy)]
enum DefineBuilderKind {
    Service,
    Contributor,
}

impl DefineBuilderKind {
    fn anchor_substring(self) -> &'static str {
        match self {
            Self::Service => ".service::<",
            Self::Contributor => ".contribute(",
        }
    }
    fn call(self, pascal: &str) -> String {
        match self {
            Self::Service => format!(".service::<{pascal}>()"),
            Self::Contributor => format!(".contribute({pascal})"),
        }
    }
}

/// Insert a `use <name>::<Pascal>;` + the appropriate builder call
/// into the parent module's `mod.rs`. Idempotent against re-runs.
fn try_register_in_define_builder(
    module_mod_rs: &Path,
    snake: &str,
    pascal: &str,
    kind: DefineBuilderKind,
) -> Result<RegisterOutcome, GenerateError> {
    let mut contents = fs::read_to_string(module_mod_rs).map_err(|e| GenerateError::Io {
        path: module_mod_rs.to_path_buf(),
        source: e,
    })?;

    let call = kind.call(pascal);
    if contents.contains(&call) {
        return Ok(RegisterOutcome::AlreadyRegistered);
    }

    // `use` line — add only if missing.
    let use_line = format!("use {snake}::{pascal};");
    if !contents.lines().any(|l| l.trim() == use_line) {
        insert_use_after_last_use(&mut contents, &use_line);
    }

    let inserted = insert_chain_call_after_anchor(&mut contents, kind.anchor_substring(), &call)
        || insert_chain_call_after_anchor(&mut contents, "define_module(", &call);
    if !inserted {
        return Ok(RegisterOutcome::AnchorNotFound);
    }

    fs::write(module_mod_rs, contents).map_err(|e| GenerateError::Io {
        path: module_mod_rs.to_path_buf(),
        source: e,
    })?;
    Ok(RegisterOutcome::Inserted)
}

// ────────────────────────── g contributor ────────────────────────────

/// Decoded form of the `g contributor` subcommand.
pub struct GenerateContributorArgs {
    /// `<module>/<contributor_snake>` spec.
    pub spec: String,
    /// Override the project root.
    pub project_root: Option<PathBuf>,
    /// Overwrite the contributor file if it already exists.
    pub force: bool,
    /// Try to auto-insert `use <name>::<Pascal>;` + `.contribute(Pascal)`
    /// into the parent module's `mod.rs`.
    pub auto_register: bool,
}

/// Result of `generate_contributor`.
#[derive(Debug)]
pub struct GenerateContributorResult {
    pub file: PathBuf,
    pub register: RegisterOutcome,
}

/// Run the `g contributor <module>/<contributor_snake>` flow.
pub fn generate_contributor(
    args: &GenerateContributorArgs,
) -> Result<GenerateContributorResult, GenerateError> {
    let (module, snake) = parse_kind_spec(&args.spec)?;
    let pascal = to_pascal_case(snake);

    let root = match &args.project_root {
        Some(p) => p.clone(),
        None => find_project_root(Path::new("."))?,
    };
    let module_mod_rs = root.join("src/modules").join(module).join("mod.rs");
    if !module_mod_rs.is_file() {
        return Err(GenerateError::ModuleMissing(
            root.join("src/modules").join(module),
        ));
    }

    let file = root
        .join("src/modules")
        .join(module)
        .join(format!("{snake}.rs"));
    if file.exists() && !args.force {
        return Err(GenerateError::FileExists(file));
    }

    let rendered = render_contributor(CONTRIBUTOR_TMPL, snake, &pascal);
    fs::write(&file, rendered).map_err(|e| GenerateError::Io {
        path: file.clone(),
        source: e,
    })?;

    ensure_pub_mod_line(&module_mod_rs, snake)?;

    let register = if args.auto_register {
        try_register_in_define_builder(
            &module_mod_rs,
            snake,
            &pascal,
            DefineBuilderKind::Contributor,
        )?
    } else {
        RegisterOutcome::Skipped
    };

    Ok(GenerateContributorResult { file, register })
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
            auto_register: false,
        };
        let res = generate_module(&args).unwrap();
        assert_eq!(res.module_dir, root.join("src/modules/posts"));

        let mod_rs = fs::read_to_string(res.module_dir.join("mod.rs")).unwrap();
        assert!(mod_rs.contains(r#"define_module("posts")"#));
        assert!(mod_rs.contains(r#".prefix("/posts")"#));

        let handlers_rs = fs::read_to_string(res.module_dir.join("handlers.rs")).unwrap();
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
            auto_register: false,
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
            auto_register: false,
        })
        .unwrap_err();
        assert!(matches!(err, GenerateError::ModuleExists(_)), "got {err:?}");
    }

    // ─────────────────── g service ───────────────────

    #[test]
    fn to_pascal_case_converts_snake() {
        assert_eq!(to_pascal_case("email"), "Email");
        assert_eq!(to_pascal_case("email_sender"), "EmailSender");
        assert_eq!(to_pascal_case("user_repository"), "UserRepository");
        assert_eq!(to_pascal_case("v1_handler"), "V1Handler");
        // Empty + degenerate inputs preserved (validation lives upstream).
        assert_eq!(to_pascal_case(""), "");
    }

    #[test]
    fn parse_kind_spec_splits_module_and_name() {
        assert_eq!(parse_kind_spec("users/email").unwrap(), ("users", "email"));
        assert_eq!(
            parse_kind_spec("users/email_sender").unwrap(),
            ("users", "email_sender")
        );
    }

    #[test]
    fn parse_kind_spec_rejects_bad_shapes() {
        // No slash
        assert!(matches!(
            parse_kind_spec("emailsender").unwrap_err(),
            GenerateError::InvalidSpec(_)
        ));
        // Empty halves
        assert!(matches!(
            parse_kind_spec("/email").unwrap_err(),
            GenerateError::InvalidSpec(_)
        ));
        assert!(matches!(
            parse_kind_spec("users/").unwrap_err(),
            GenerateError::InvalidSpec(_)
        ));
        // Nested slashes — we only support one-level nesting today.
        assert!(matches!(
            parse_kind_spec("users/sub/email").unwrap_err(),
            GenerateError::InvalidSpec(_)
        ));
        // Bad identifier on either side cascades to InvalidName.
        assert!(matches!(
            parse_kind_spec("Users/email").unwrap_err(),
            GenerateError::InvalidName(_)
        ));
        assert!(matches!(
            parse_kind_spec("users/Email").unwrap_err(),
            GenerateError::InvalidName(_)
        ));
    }

    fn make_skeleton_with_module(dir: &Path, module: &str) {
        make_skeleton(dir);
        fs::create_dir_all(dir.join("src/modules").join(module)).unwrap();
        fs::write(
            dir.join("src/modules").join(module).join("mod.rs"),
            format!("//! {module}\npub mod handlers;\n"),
        )
        .unwrap();
        // Append the module to the top-level mod.rs so it'd compile,
        // though the codegen here doesn't actually need this.
        let decl = format!("pub mod {module};\n");
        let mut top = fs::read_to_string(dir.join("src/modules/mod.rs")).unwrap();
        top.push_str(&decl);
        fs::write(dir.join("src/modules/mod.rs"), top).unwrap();
    }

    #[test]
    fn generate_service_writes_file_and_appends_module_mod() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_module(&root, "users");

        let res = generate_service(&GenerateServiceArgs {
            spec: "users/email_sender".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: false,
        })
        .unwrap();

        assert_eq!(res.file, root.join("src/modules/users/email_sender.rs"));

        let body = fs::read_to_string(&res.file).unwrap();
        assert!(body.contains("pub struct EmailSender"));
        assert!(body.contains(r#""email_sender ready""#));

        let mod_rs = fs::read_to_string(root.join("src/modules/users/mod.rs")).unwrap();
        assert_eq!(
            mod_rs.matches("pub mod email_sender;").count(),
            1,
            "expected one append in module mod.rs: {mod_rs}"
        );
    }

    #[test]
    fn generate_service_refuses_when_parent_module_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton(&root);
        // No `users` module — only the default hello one from make_skeleton.

        let err = generate_service(&GenerateServiceArgs {
            spec: "users/email_sender".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: false,
        })
        .unwrap_err();
        assert!(
            matches!(err, GenerateError::ModuleMissing(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn generate_service_refuses_existing_file_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_module(&root, "users");
        fs::write(
            root.join("src/modules/users/email_sender.rs"),
            "// user wrote this",
        )
        .unwrap();

        let err = generate_service(&GenerateServiceArgs {
            spec: "users/email_sender".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: false,
        })
        .unwrap_err();
        assert!(matches!(err, GenerateError::FileExists(_)), "got {err:?}");
    }

    #[test]
    fn generate_service_force_overwrites_but_append_stays_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_module(&root, "users");

        let args = GenerateServiceArgs {
            spec: "users/email_sender".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: false,
        };
        generate_service(&args).unwrap();

        let force_args = GenerateServiceArgs {
            spec: "users/email_sender".into(),
            project_root: Some(root.clone()),
            force: true,
            auto_register: false,
        };
        generate_service(&force_args).unwrap();

        let mod_rs = fs::read_to_string(root.join("src/modules/users/mod.rs")).unwrap();
        assert_eq!(
            mod_rs.matches("pub mod email_sender;").count(),
            1,
            "double-append on second generate: {mod_rs}"
        );
    }

    // ────────────── g contributor ──────────────

    #[test]
    fn generate_contributor_writes_file_and_appends_module_mod() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_module(&root, "users");

        let res = generate_contributor(&GenerateContributorArgs {
            spec: "users/load_current_user".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: false,
        })
        .unwrap();

        assert_eq!(
            res.file,
            root.join("src/modules/users/load_current_user.rs")
        );

        let body = fs::read_to_string(&res.file).unwrap();
        // PascalCase derived from snake_case for both the contributor
        // fn and its output struct.
        assert!(body.contains("pub async fn LoadCurrentUser("));
        assert!(body.contains("pub struct LoadCurrentUserOut"));
        // The macro-driven sugar shows up — adopters get a working
        // skeleton that compiles after `cargo kick g`.
        assert!(body.contains("#[contributor]"));

        let mod_rs = fs::read_to_string(root.join("src/modules/users/mod.rs")).unwrap();
        assert_eq!(
            mod_rs.matches("pub mod load_current_user;").count(),
            1,
            "expected one append in module mod.rs: {mod_rs}"
        );
    }

    #[test]
    fn generate_contributor_refuses_when_parent_module_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton(&root);

        let err = generate_contributor(&GenerateContributorArgs {
            spec: "users/load_current_user".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: false,
        })
        .unwrap_err();
        assert!(
            matches!(err, GenerateError::ModuleMissing(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn generate_contributor_refuses_existing_file_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_module(&root, "users");
        fs::write(
            root.join("src/modules/users/load_current_user.rs"),
            "// user wrote this",
        )
        .unwrap();

        let err = generate_contributor(&GenerateContributorArgs {
            spec: "users/load_current_user".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: false,
        })
        .unwrap_err();
        assert!(matches!(err, GenerateError::FileExists(_)), "got {err:?}");
    }

    #[test]
    fn generate_contributor_force_keeps_append_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_module(&root, "users");

        generate_contributor(&GenerateContributorArgs {
            spec: "users/load_current_user".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: false,
        })
        .unwrap();
        generate_contributor(&GenerateContributorArgs {
            spec: "users/load_current_user".into(),
            project_root: Some(root.clone()),
            force: true,
            auto_register: false,
        })
        .unwrap();

        let mod_rs = fs::read_to_string(root.join("src/modules/users/mod.rs")).unwrap();
        assert_eq!(
            mod_rs.matches("pub mod load_current_user;").count(),
            1,
            "double-append on second generate: {mod_rs}"
        );
    }

    // ────────────── auto-register paths ──────────────

    /// Make a more realistic project that has both `src/main.rs` (with
    /// a bootstrap chain) and a module's `mod.rs` (with a define()
    /// builder) — covers both auto-register targets.
    fn make_skeleton_with_main_and_define(dir: &Path, module: &str) {
        make_skeleton_with_module(dir, module);
        // Overwrite the module mod.rs with one that has a proper
        // define() chain so .service::<...>() / .contribute(...) can be
        // inserted into a real chain.
        fs::write(
            dir.join("src/modules").join(module).join("mod.rs"),
            format!(
                "//! `{module}` resource.\n\
                 pub mod handlers;\n\
                 \n\
                 use kick_rs::{{define_module, Module}};\n\
                 \n\
                 pub fn define() -> Module {{\n    \
                     define_module(\"{module}\")\n        \
                         .prefix(\"/{module}\")\n        \
                         .build()\n\
                 }}\n",
            ),
        )
        .unwrap();
        fs::write(
            dir.join("src/main.rs"),
            "use kick_rs::{bootstrap, KickResult};\n\
             mod modules;\n\
             \n\
             #[tokio::main]\n\
             async fn main() -> KickResult<()> {\n    \
                 bootstrap()\n        \
                     .listen(\"0.0.0.0:3000\")\n        \
                     .await\n\
             }\n",
        )
        .unwrap();
    }

    #[test]
    fn generate_module_auto_registers_in_main() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_main_and_define(&root, "users");

        let res = generate_module(&GenerateModuleArgs {
            name: "posts".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: true,
        })
        .unwrap();
        assert_eq!(res.register, RegisterOutcome::Inserted);

        let main_rs = fs::read_to_string(root.join("src/main.rs")).unwrap();
        assert!(
            main_rs.contains("    .module(modules::posts::define())"),
            "got: {main_rs}"
        );
    }

    #[test]
    fn generate_module_auto_register_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_main_and_define(&root, "users");

        // First pass inserts.
        let first = generate_module(&GenerateModuleArgs {
            name: "posts".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: true,
        })
        .unwrap();
        assert_eq!(first.register, RegisterOutcome::Inserted);

        // Second pass with --force: should detect the existing
        // registration and skip the re-insert.
        let second = generate_module(&GenerateModuleArgs {
            name: "posts".into(),
            project_root: Some(root.clone()),
            force: true,
            auto_register: true,
        })
        .unwrap();
        assert_eq!(second.register, RegisterOutcome::AlreadyRegistered);

        let main_rs = fs::read_to_string(root.join("src/main.rs")).unwrap();
        assert_eq!(
            main_rs.matches(".module(modules::posts::define())").count(),
            1
        );
    }

    #[test]
    fn generate_service_auto_registers_use_and_call() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_main_and_define(&root, "users");

        let res = generate_service(&GenerateServiceArgs {
            spec: "users/email_sender".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: true,
        })
        .unwrap();
        assert_eq!(res.register, RegisterOutcome::Inserted);

        let mod_rs = fs::read_to_string(root.join("src/modules/users/mod.rs")).unwrap();
        assert!(
            mod_rs.contains("use email_sender::EmailSender;"),
            "missing use line: {mod_rs}"
        );
        assert!(
            mod_rs.contains(".service::<EmailSender>()"),
            "missing service call: {mod_rs}"
        );
    }

    #[test]
    fn generate_contributor_auto_registers_use_and_call() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton_with_main_and_define(&root, "users");

        let res = generate_contributor(&GenerateContributorArgs {
            spec: "users/load_current_user".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: true,
        })
        .unwrap();
        assert_eq!(res.register, RegisterOutcome::Inserted);

        let mod_rs = fs::read_to_string(root.join("src/modules/users/mod.rs")).unwrap();
        assert!(
            mod_rs.contains("use load_current_user::LoadCurrentUser;"),
            "missing use line: {mod_rs}"
        );
        assert!(
            mod_rs.contains(".contribute(LoadCurrentUser)"),
            "missing contribute call: {mod_rs}"
        );
    }

    #[test]
    fn generate_module_falls_back_when_no_main_rs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        make_skeleton(&root); // no src/main.rs

        let res = generate_module(&GenerateModuleArgs {
            name: "posts".into(),
            project_root: Some(root.clone()),
            force: false,
            auto_register: true,
        })
        .unwrap();
        // No main.rs => caller gets a clear signal to print the manual hint.
        assert_eq!(res.register, RegisterOutcome::TargetMissing);
        // File emission and pub-mod-append still happened.
        assert!(root.join("src/modules/posts/mod.rs").is_file());
    }
}
