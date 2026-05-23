#![doc = include_str!("../README.md")]
//
// (The `pub` modules below are the library half of `kick-rs-cli` —
// the `cargo-kick` binary is a thin clap shell over them, and
// integration tests use them directly. Adopters install the CLI via
// `cargo install kick-rs-cli`; they don't depend on the library.)
#![forbid(unsafe_code)]
#![warn(rust_2018_idioms)]

pub mod add;
pub mod dev;
pub mod generate;
pub mod info;
pub mod new;
pub mod register;
pub mod templates;
