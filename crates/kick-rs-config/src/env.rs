//! Environment-variable + dotenv-file handling.
//!
//! Two responsibilities, kept separate:
//!
//! 1. **Dotenv loader** — parse a `KEY=VALUE`-format file and write the
//!    entries into `std::env`. `kick-rs-config` is intentionally the
//!    only place that mutates the process env on the adopter's behalf.
//! 2. **Prefixed env reader** — walk `std::env::vars()`, keep those
//!    starting with the prefix, and build a nested `serde_json::Value`
//!    by interpreting `__` as a path separator.

use kick_rs_core::{KickError, KickResult};
use serde_json::{Map, Value};
use std::path::Path;

/// Parse `path` as a `.env` file and `set_var` each entry it produces.
///
/// Variables *already set* in the environment are not overwritten —
/// that matches dotenv-rs semantics and means a CI-injected env wins
/// over a checked-in `.env`.
pub(crate) fn load_dotenv(path: &Path, optional: bool) -> KickResult<()> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && optional => return Ok(()),
        Err(e) => {
            return Err(KickError::new(
                "RK_C_IO",
                format!("could not read dotenv {}: {e}", path.display()),
            ));
        }
    };

    for (lineno, raw) in content.lines().enumerate() {
        let line = strip_inline_comment(raw.trim());
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(KickError::new(
                "RK_C_PARSE",
                format!(
                    "dotenv {} line {}: expected KEY=VALUE",
                    path.display(),
                    lineno + 1
                ),
            ));
        };
        let key = key.trim();
        let value = unquote(value.trim());
        if key.is_empty() {
            return Err(KickError::new(
                "RK_C_PARSE",
                format!("dotenv {} line {}: empty key", path.display(), lineno + 1),
            ));
        }
        if std::env::var_os(key).is_none() {
            // Existing env wins — CI / direct exports beat a checked-in .env.
            // SAFETY: std::env::set_var is sound; the unsafety guidance in
            // Rust 1.78+ is about concurrent reads in multi-threaded
            // programs. Config loading happens early in main(), single-threaded.
            std::env::set_var(key, value);
        }
    }

    Ok(())
}

/// Read `std::env::vars()` and synthesize a nested `serde_json::Value`
/// from variables prefixed with `prefix`. The prefix is stripped, the
/// remainder lowercased, and `__` is interpreted as a path separator.
pub(crate) fn read_env_with_prefix(prefix: &str) -> Value {
    let mut root = Map::new();
    for (k, v) in std::env::vars() {
        let Some(rest) = k.strip_prefix(prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let lowered = rest.to_lowercase();
        let path: Vec<&str> = lowered.split("__").collect();
        insert_path(&mut root, &path, Value::String(v));
    }
    Value::Object(root)
}

fn insert_path(into: &mut Map<String, Value>, path: &[&str], leaf: Value) {
    let (head, tail) = match path.split_first() {
        Some(s) => s,
        None => return,
    };
    if tail.is_empty() {
        into.insert((*head).to_string(), leaf);
        return;
    }
    let slot = into
        .entry((*head).to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    // If a parent path collides with a non-object that arrived first,
    // overwrite it — last-write-wins is consistent with deep_merge.
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    if let Value::Object(child) = slot {
        insert_path(child, tail, leaf);
    }
}

fn strip_inline_comment(s: &str) -> &str {
    // Crude but matches dotenv convention: `#` outside of any quoted
    // value starts a comment. We only honor `#` when it's preceded by
    // whitespace or starts the line, to avoid mangling values that
    // contain `#`.
    let mut prev_ws = true;
    for (i, c) in s.char_indices() {
        if c == '#' && prev_ws {
            return s[..i].trim_end();
        }
        prev_ws = c.is_whitespace();
    }
    s
}

fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_owned()
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn write_tmp(contents: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        f
    }

    #[test]
    fn dotenv_loads_keys_and_skips_comments() {
        // Use a unique prefix per-test so we don't clash with other tests
        // running in parallel (cargo test uses a thread pool).
        let f = write_tmp(
            r#"
# leading comment
PHASE9_A=alpha
PHASE9_B = "beta"          # trailing comment
PHASE9_C='gamma'
"#,
        );
        load_dotenv(f.path(), false).unwrap();
        assert_eq!(std::env::var("PHASE9_A").unwrap(), "alpha");
        assert_eq!(std::env::var("PHASE9_B").unwrap(), "beta");
        assert_eq!(std::env::var("PHASE9_C").unwrap(), "gamma");
    }

    #[test]
    fn dotenv_does_not_override_existing_env() {
        std::env::set_var("PHASE9_PREEXISTING", "real-value");
        let f = write_tmp("PHASE9_PREEXISTING=should-not-win\n");
        load_dotenv(f.path(), false).unwrap();
        assert_eq!(std::env::var("PHASE9_PREEXISTING").unwrap(), "real-value");
    }

    #[test]
    fn env_prefix_nests_on_double_underscore() {
        std::env::set_var("PHASE9N_DB__URL", "postgres://x");
        std::env::set_var("PHASE9N_DB__POOL_SIZE", "10");
        std::env::set_var("PHASE9N_PORT", "3000");

        let v = read_env_with_prefix("PHASE9N_");
        assert_eq!(
            v,
            json!({
                "db": { "url": "postgres://x", "pool_size": "10" },
                "port": "3000",
            })
        );
    }

    #[test]
    fn dotenv_missing_optional_is_fine() {
        load_dotenv(Path::new("does-not-exist.env"), true).unwrap();
    }

    #[test]
    fn dotenv_missing_required_errors() {
        let err = load_dotenv(Path::new("does-not-exist.env"), false).unwrap_err();
        assert_eq!(err.code, "RK_C_IO");
    }
}
