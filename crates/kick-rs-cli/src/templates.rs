//! Compile-time-embedded starter templates for `cargo kick new`.
//!
//! Each entry is `(relative_path, contents)`. Templates use a tiny
//! `{{var}}` substitution scheme — see [`render`] — keyed off the
//! variables in [`Vars`]. Anything that isn't a known variable is
//! left untouched, so the template files are still valid Rust /
//! TOML for editor tooling.

/// Variables the templates can reference. Kept tiny on purpose — the
/// scaffold should be readable code with one or two substitutions, not
/// a templating-engine dialect.
pub struct Vars<'a> {
    /// Raw name as supplied on the CLI (kebab-case, e.g. "my-app").
    /// Used in `Cargo.toml`'s `name`, README headings, etc.
    pub project_name: &'a str,
    /// Snake-cased form of [`Self::project_name`] for Rust crate
    /// identifiers and tracing-subscriber log targets.
    pub project_name_snake: &'a str,
}

/// Manifest of every file the `new` scaffold emits. Tuples of
/// `(destination_relative_path, embedded_template_contents)`.
///
/// Paths are written as-is into the project directory. They include
/// `Cargo.toml`, `src/...`, dotfiles like `.gitignore`. No path
/// substitution — the only thing template-substituted is the file
/// *contents*.
pub const FILES: &[(&str, &str)] = &[
    (
        "Cargo.toml",
        include_str!("../templates/new/Cargo.toml.tmpl"),
    ),
    (
        "src/main.rs",
        include_str!("../templates/new/src/main.rs.tmpl"),
    ),
    (
        "src/modules/mod.rs",
        include_str!("../templates/new/src/modules/mod.rs.tmpl"),
    ),
    (
        "src/modules/hello/mod.rs",
        include_str!("../templates/new/src/modules/hello/mod.rs.tmpl"),
    ),
    (
        "src/modules/hello/handlers.rs",
        include_str!("../templates/new/src/modules/hello/handlers.rs.tmpl"),
    ),
    (
        ".env.example",
        include_str!("../templates/new/.env.example.tmpl"),
    ),
    (
        ".gitignore",
        include_str!("../templates/new/.gitignore.tmpl"),
    ),
    ("README.md", include_str!("../templates/new/README.md.tmpl")),
];

/// Substitute `{{project_name}}` and `{{project_name_snake}}` into the
/// given template. Unknown `{{...}}` sequences are passed through
/// untouched.
pub fn render(template: &str, vars: &Vars<'_>) -> String {
    template
        .replace("{{project_name_snake}}", vars.project_name_snake)
        .replace("{{project_name}}", vars.project_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_substitutes_known_vars() {
        let v = Vars {
            project_name: "my-app",
            project_name_snake: "my_app",
        };
        let s = render(
            "name = \"{{project_name}}\"\ntarget = \"{{project_name_snake}}\"",
            &v,
        );
        assert_eq!(s, "name = \"my-app\"\ntarget = \"my_app\"");
    }

    #[test]
    fn render_leaves_unknown_vars_alone() {
        let v = Vars {
            project_name: "x",
            project_name_snake: "x",
        };
        let s = render("{{unknown_thing}}", &v);
        assert_eq!(s, "{{unknown_thing}}");
    }

    #[test]
    fn file_manifest_is_non_empty_and_relative() {
        assert!(!FILES.is_empty(), "scaffold must emit at least one file");
        for (path, _) in FILES {
            assert!(
                !path.starts_with('/') && !path.contains(".."),
                "manifest paths must be project-relative without traversal: {path}"
            );
        }
    }
}
