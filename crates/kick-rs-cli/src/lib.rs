//! Library half of `kick-rs-cli`. The `cargo-kick` binary is a thin
//! clap shell over these modules; integration tests use them directly.
//!
//! Public surface is small on purpose — this isn't a crate adopters
//! consume, it's a place to put logic that's awkward to test through
//! the binary's stdout/stderr.

#![forbid(unsafe_code)]
#![warn(rust_2018_idioms)]

pub mod new;
pub mod templates;
