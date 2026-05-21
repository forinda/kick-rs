//! # rustkick-macros
//!
//! Opt-in proc-macro sugar for rustkick. Every macro here expands to a
//! call you could write yourself against [`rustkick_core`]. See
//! [`SPEC.md` §8 / Macro Expansion Strategy](../ARCHITECTURE.md#8-macro-expansion-strategy)
//! for the planned expansions.
//!
//! ```ignore
//! #[rustkick::service]
//! pub struct UserService { repo: Inject<UserRepository> }
//! ```

use proc_macro::TokenStream;

/// `#[service]` — derive DI registration helpers for a struct.
///
/// Phase-3 macro: today this is a pass-through so the crate compiles and
/// the public macro name is reserved.
#[proc_macro_attribute]
pub fn service(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// `#[handler]` — opt-in route registration hook.
///
/// Phase-3 macro: pass-through for now.
#[proc_macro_attribute]
pub fn handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
