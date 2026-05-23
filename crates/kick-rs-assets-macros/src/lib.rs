//! Proc-macro half of `kick-rs-assets`. Ships `embed_assets!`.
//!
//! The macro reads a directory at compile time and emits a static
//! tree of `kick_rs_assets::EmbeddedAssets` referencing every file's
//! contents via `include_bytes!`. The emitted code references the
//! runtime types through `kick_rs_assets` (resolved via
//! `proc-macro-crate`), so adopters never need to depend on
//! `include_dir` or any other vendored crate themselves.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote};
use std::path::{Path, PathBuf};
use syn::{parse_macro_input, LitStr};

/// `embed_assets!("$CARGO_MANIFEST_DIR/dist")` — emit a static
/// `EmbeddedAssets` containing every file under the directory.
///
/// Path expansion:
/// - The string is read as-is for absolute paths.
/// - `$CARGO_MANIFEST_DIR` and `$OUT_DIR` are substituted from
///   environment vars at macro expansion time (the standard
///   include_dir convention; we accept the same syntax so the move
///   is invisible).
/// - Relative paths are resolved against `$CARGO_MANIFEST_DIR`.
#[proc_macro]
pub fn embed_assets(input: TokenStream) -> TokenStream {
    let lit: LitStr = parse_macro_input!(input);
    let raw = lit.value();
    let path = match expand_path(&raw) {
        Ok(p) => p,
        Err(e) => {
            return syn::Error::new(lit.span(), e).to_compile_error().into();
        }
    };
    if !path.is_dir() {
        return syn::Error::new(
            lit.span(),
            format!("`{}` is not a directory", path.display()),
        )
        .to_compile_error()
        .into();
    }

    let krate = resolve_kick_assets_path();
    let body = match emit_dir(&krate, &path, &path) {
        Ok(ts) => ts,
        Err(e) => {
            return syn::Error::new(lit.span(), e).to_compile_error().into();
        }
    };

    quote!({ #body }).into()
}

/// Expand `$CARGO_MANIFEST_DIR` / `$OUT_DIR` references and turn the
/// (possibly relative) path into an absolute `PathBuf`.
fn expand_path(s: &str) -> Result<PathBuf, String> {
    let expanded = match s.strip_prefix("$CARGO_MANIFEST_DIR") {
        Some(rest) => {
            let base = std::env::var("CARGO_MANIFEST_DIR")
                .map_err(|_| "$CARGO_MANIFEST_DIR is not set".to_owned())?;
            let mut p = PathBuf::from(base);
            // The rest starts with `/` or `\` — push it as a relative
            // segment, stripping the separator first.
            p.push(rest.trim_start_matches(['/', '\\']));
            p
        }
        None => match s.strip_prefix("$OUT_DIR") {
            Some(rest) => {
                let base =
                    std::env::var("OUT_DIR").map_err(|_| "$OUT_DIR is not set".to_owned())?;
                let mut p = PathBuf::from(base);
                p.push(rest.trim_start_matches(['/', '\\']));
                p
            }
            None => {
                let p = PathBuf::from(s);
                if p.is_absolute() {
                    p
                } else {
                    let base = std::env::var("CARGO_MANIFEST_DIR")
                        .map_err(|_| "$CARGO_MANIFEST_DIR is not set".to_owned())?;
                    PathBuf::from(base).join(p)
                }
            }
        },
    };
    Ok(expanded)
}

/// Walk `dir` and emit:
///
/// ```ignore
/// kick_rs_assets::EmbeddedAssets::__new(
///     "rel/path",
///     &[
///         kick_rs_assets::EmbeddedEntry::File(kick_rs_assets::EmbeddedFile::__new(
///             "rel/path/file.js",
///             include_bytes!("/abs/path/to/file.js"),
///         )),
///         kick_rs_assets::EmbeddedEntry::Dir(/* recursive */),
///         ...
///     ],
/// )
/// ```
fn emit_dir(krate: &TokenStream2, root: &Path, dir: &Path) -> Result<TokenStream2, String> {
    let rel = relative_to_root(root, dir);
    let rel_lit = LitStr::new(&rel, proc_macro2::Span::call_site());

    let mut entries: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .map_err(|e| format!("could not read dir `{}`: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (name, e.path())
        })
        .collect();
    // Stable order so the emitted tree is deterministic.
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut emitted = Vec::with_capacity(entries.len());
    for (_, path) in entries {
        if path.is_file() {
            let file_rel = relative_to_root(root, &path);
            let file_rel_lit = LitStr::new(&file_rel, proc_macro2::Span::call_site());
            // `include_bytes!` needs the absolute (or build-cwd-relative) path —
            // we pass the absolute path string.
            let abs = path.to_string_lossy().into_owned();
            let abs_lit = LitStr::new(&abs, proc_macro2::Span::call_site());
            emitted.push(quote! {
                #krate::EmbeddedEntry::File(#krate::EmbeddedFile::__new(
                    #file_rel_lit,
                    ::core::include_bytes!(#abs_lit),
                ))
            });
        } else if path.is_dir() {
            let sub = emit_dir(krate, root, &path)?;
            emitted.push(quote! { #krate::EmbeddedEntry::Dir(#sub) });
        }
        // Skip symlinks / other special files — embedding them across
        // platforms is risky.
    }

    Ok(quote! {
        #krate::EmbeddedAssets::__new(
            #rel_lit,
            &[ #(#emitted),* ],
        )
    })
}

fn relative_to_root(root: &Path, path: &Path) -> String {
    let p = path.strip_prefix(root).unwrap_or(path);
    // Normalize separators — emitted strings on Windows would otherwise
    // contain backslashes and break `get_file("foo/bar.js")` lookups.
    p.to_string_lossy().replace('\\', "/")
}

/// Path under which the macro's expansion references kick-rs-assets'
/// runtime types. Tries `kick-rs-assets` first; falls back to the
/// umbrella's hidden alias when adopters depend only on `kick-rs`.
fn resolve_kick_assets_path() -> TokenStream2 {
    if let Ok(found) = crate_name("kick-rs-assets") {
        return path_for(found);
    }
    if let Ok(found) = crate_name("kick-rs") {
        // Umbrella users get this via `pub use kick_rs_assets as assets`
        // — when the `assets` feature is enabled. The macro's emitted
        // path then routes through `kick_rs::assets`.
        let base = path_for(found);
        return quote! { #base::assets };
    }
    // Best-effort fallback — emit the absolute path so the compiler's
    // "cannot find crate" error points adopters at the missing dep.
    quote! { ::kick_rs_assets }
}

fn path_for(found: FoundCrate) -> TokenStream2 {
    match found {
        FoundCrate::Itself => quote! { crate },
        FoundCrate::Name(name) => {
            let ident = format_ident!("{}", name);
            quote! { ::#ident }
        }
    }
}
