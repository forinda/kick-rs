//! End-to-end test for `cargo kick new`.
//!
//! Scaffolds a project into a tempdir, points its `kick-rs` dep at the
//! local workspace via a `[patch.crates-io]` block, then runs
//! `cargo check` to prove the generated code compiles.
//!
//! `cargo check` is expensive (downloads + compiles deps for a fresh
//! crate), so this test is `#[ignore]`d by default. Run it explicitly
//! with `cargo test -p kick-rs-cli -- --ignored` when changing the
//! scaffold templates.

use std::process::Command;

fn workspace_root() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR points at the kick-rs-cli crate; the
    // workspace root is two `..` above.
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_owned()
}

#[test]
#[ignore = "runs cargo check on a freshly scaffolded project; opt in with --ignored"]
fn scaffolded_project_compiles() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("scaffold-test");

    // Run our own binary's `new` flow as a library — same code path as
    // `cargo kick new` minus the clap shell.
    let args = kick_rs_cli::new::NewArgs {
        name: "scaffold-test".into(),
        path: Some(target.clone()),
        force: false,
    };
    // Note: `new::NewArgs` has no auto_register field — auto-register
    // is a `generate::*` concern, not a scaffold concern.
    kick_rs_cli::new::run(&args).expect("scaffold failed");

    // Point the scaffold at the *local* workspace crates so we test
    // the in-progress framework code, not whatever's on crates.io.
    let workspace = workspace_root();
    let patch = format!(
        "\n[patch.crates-io]\n\
         kick-rs        = {{ path = \"{root}/crates/kick-rs\" }}\n\
         kick-rs-core   = {{ path = \"{root}/crates/kick-rs-core\" }}\n\
         kick-rs-http   = {{ path = \"{root}/crates/kick-rs-http\" }}\n\
         kick-rs-macros = {{ path = \"{root}/crates/kick-rs-macros\" }}\n\
         kick-rs-config = {{ path = \"{root}/crates/kick-rs-config\" }}\n",
        root = workspace.display().to_string().replace('\\', "/"),
    );
    let cargo_toml = target.join("Cargo.toml");
    let mut existing = std::fs::read_to_string(&cargo_toml).unwrap();
    existing.push_str(&patch);
    std::fs::write(&cargo_toml, existing).unwrap();

    let status = Command::new(env!("CARGO"))
        .arg("check")
        .arg("--quiet")
        .current_dir(&target)
        .status()
        .expect("could not invoke cargo");
    assert!(status.success(), "scaffolded project failed to compile");
}
