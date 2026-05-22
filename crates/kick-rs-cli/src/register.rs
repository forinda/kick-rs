//! Conservative text-level edits to wire generated code into the
//! existing builder chains.
//!
//! Why not syn? Re-emitting Rust through `syn` + `prettyplease` loses
//! formatting, comments, and blank-line layout — adopters then see a
//! surprise diff every time they generate something. So this module
//! takes a narrower approach: find a known anchor substring, insert
//! one line, leave everything else alone.
//!
//! The patterns we match are the exact ones the `new` scaffold emits
//! plus what the users-api example demonstrates. Anything more
//! creative (extracted-helper bootstrap, custom builder wrapper) is
//! out of scope — the callers fall back to printing the manual hint.

/// Result of one auto-register attempt. The wrapping caller turns
/// these into print lines.
#[derive(Debug, PartialEq, Eq)]
pub enum RegisterOutcome {
    /// Edit applied and written.
    Inserted,
    /// Target file already contains the registration — nothing to do.
    AlreadyRegistered,
    /// No suitable anchor found; caller falls back to printing a hint.
    AnchorNotFound,
    /// Target file doesn't exist (e.g. no `src/main.rs`).
    TargetMissing,
    /// The user opted out via `--no-register`; we didn't attempt the
    /// edit at all. Kept distinct from `AnchorNotFound` so the CLI
    /// can phrase the message correctly.
    Skipped,
}

/// Insert `new_call` (a builder method like `.module(modules::posts::define())`)
/// into `contents` at the right indentation, anchored to the *last*
/// line containing `anchor_substring`.
///
/// Indentation rule:
/// - If the anchor line (trimmed) starts with `.`, it's itself a chain
///   element → the new line uses the *same* leading whitespace.
/// - Otherwise the anchor is the chain opener (`bootstrap()`,
///   `define_module(...)`) → the new line uses the opener's
///   whitespace + 4 spaces.
///
/// Returns true if an anchor was found and `contents` mutated.
pub fn insert_chain_call_after_anchor(
    contents: &mut String,
    anchor_substring: &str,
    new_call: &str,
) -> bool {
    let lines: Vec<&str> = contents.lines().collect();
    let pos = lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, l)| l.contains(anchor_substring))
        .map(|(i, _)| i);
    let Some(i) = pos else {
        return false;
    };

    let anchor = lines[i];
    let anchor_ws: String = anchor.chars().take_while(|c| c.is_whitespace()).collect();
    let trimmed = anchor.trim_start();
    let indent = if trimmed.starts_with('.') {
        anchor_ws
    } else {
        format!("{anchor_ws}    ")
    };
    let new_line = format!("{indent}{new_call}");

    let mut new_lines: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    new_lines.insert(i + 1, new_line);
    *contents = new_lines.join("\n");
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    true
}

/// Insert a `use ...;` line just after the *last* existing `use ...;`
/// line at the top of the file. Returns true if inserted, false if no
/// `use` lines were found.
pub fn insert_use_after_last_use(contents: &mut String, new_use: &str) -> bool {
    let lines: Vec<&str> = contents.lines().collect();
    let pos = lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, l)| l.trim_start().starts_with("use "))
        .map(|(i, _)| i);
    let Some(i) = pos else {
        return false;
    };

    let mut new_lines: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    new_lines.insert(i + 1, new_use.to_string());
    *contents = new_lines.join("\n");
    if !contents.ends_with('\n') {
        contents.push('\n');
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_insert_after_opener_indents_plus_four() {
        let mut src = "bootstrap()\n    .listen(addr)\n    .await\n".to_string();
        let inserted =
            insert_chain_call_after_anchor(&mut src, "bootstrap()", ".module(foo::define())");
        assert!(inserted);
        // Opener has 0 ws; new chain element should have 4 ws.
        assert!(
            src.contains("\n    .module(foo::define())\n    .listen"),
            "got: {src}"
        );
    }

    #[test]
    fn chain_insert_after_chain_element_keeps_same_indent() {
        let mut src =
            "bootstrap()\n    .module(a::define())\n    .listen(addr)\n    .await\n".to_string();
        let inserted = insert_chain_call_after_anchor(&mut src, ".module(", ".module(b::define())");
        assert!(inserted);
        // Anchor `.module(a::define())` is itself a chain element at 4 ws;
        // new line should also be 4 ws and land *after* the last `.module`.
        assert!(
            src.contains("    .module(a::define())\n    .module(b::define())\n    .listen"),
            "got: {src}"
        );
    }

    #[test]
    fn chain_insert_returns_false_when_anchor_missing() {
        let mut src = "fn main() {}\n".to_string();
        let inserted = insert_chain_call_after_anchor(&mut src, "bootstrap()", ".module(x)");
        assert!(!inserted);
        assert_eq!(src, "fn main() {}\n");
    }

    #[test]
    fn chain_insert_picks_last_anchor_when_multiple() {
        let mut src =
            "bootstrap()\n    .module(a)\n    .module(b)\n    .listen(addr)\n".to_string();
        insert_chain_call_after_anchor(&mut src, ".module(", ".module(c)");
        // Inserted after the LAST `.module(`, which is `.module(b)`.
        assert!(
            src.contains("    .module(b)\n    .module(c)\n    .listen"),
            "got: {src}"
        );
    }

    #[test]
    fn use_insert_after_last_use_line() {
        let mut src = "use std::sync::Arc;\nuse kick_rs::*;\n\nfn main() {}\n".to_string();
        let inserted = insert_use_after_last_use(&mut src, "use foo::Foo;");
        assert!(inserted);
        // Lands after the last `use` line, before the blank line.
        assert!(
            src.contains("use kick_rs::*;\nuse foo::Foo;\n\nfn main()"),
            "got: {src}"
        );
    }

    #[test]
    fn use_insert_returns_false_when_no_use_lines() {
        let mut src = "fn main() {}\n".to_string();
        let inserted = insert_use_after_last_use(&mut src, "use foo::Foo;");
        assert!(!inserted);
        assert_eq!(src, "fn main() {}\n");
    }
}
