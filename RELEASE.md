# Releasing kick-rs

> How versions get cut, where they're published, and what to do when
> something goes wrong. Operational reference for maintainers — adopters
> should read [`README.md`](./README.md) for install instructions instead.

---

## TL;DR

1. Merge feature PRs into `main`. Use **Conventional Commits**
   (`feat:`, `fix:`, `docs:` etc.) — release-plz reads them.
2. The `release-plz` GitHub Action opens a **Release PR** ([example](#)) with
   the proposed version bumps + CHANGELOG entries.
3. Review, edit if needed, then merge the Release PR.
4. The same workflow publishes to crates.io and pushes git tags.

That's it. Everything below is the why + the failure modes.

---

## Versioning model

**Independent versioning per crate**, mirroring `tokio-*` and `tower-*`.
`kick-rs-core` 0.2.0 can ship while `kick-rs-http` stays at 0.1.3.

| Crate              | Releasable today | Reason                                          |
|--------------------|------------------|-------------------------------------------------|
| `kick-rs-core`     | yes              | Real impl, 36 tests passing                     |
| `kick-rs-http`     | yes              | Real impl, 9 tests passing                      |
| `kick-rs-macros`   | yes              | `#[service]` proc-macro, 5 integration tests    |
| `kick-rs`          | yes              | Thin umbrella; `macros` feature opt-in          |
| `kick-rs-config`   | **no**           | Placeholder                                     |
| `kick-rs-assets`   | **no**           | Placeholder                                     |
| `kick-rs-cli`      | **no**           | Placeholder binary                              |

Placeholder crates carry `release = false` in
[`release-plz.toml`](./release-plz.toml) **and** `publish = false` in
their `Cargo.toml` (defense in depth — can't be accidentally cut even
if release-plz is misconfigured).

The umbrella `kick-rs` crate's dependencies on `kick-rs-core` and
`kick-rs-http` are pinned by **caret** version (`^0.x.y`), and
release-plz updates those pins automatically when either of the
underlying crates ships.

## Conventional Commits — the contract

release-plz reads commit subjects to decide the next version. Stick to:

| Prefix         | Bump          |
|----------------|---------------|
| `feat(...)`    | minor         |
| `fix(...)`     | patch         |
| `perf(...)`    | patch         |
| `refactor(...)`| patch         |
| `docs(...)`    | patch (or skip if changelog-only) |
| `chore(...)`   | patch (or skip) |
| `test(...)`    | skip          |
| `ci(...)`      | skip          |
| `build(...)`   | skip          |
| `BREAKING CHANGE:` in body, or `feat!:` / `fix!:` | major (still 0.x → 0.x+1 while pre-1.0) |

Scope the prefix to the affected crate so changelog grouping works:

```
feat(core): add request-scoped resolution
fix(http): close listener on bind failure
docs(readme): update install snippets
```

Commits without a recognized prefix get skipped (no version bump, no
changelog entry). That's fine for one-off chores; a release won't be
proposed until something meaningful lands.

## Pre-v0.1.0 stage

While the API is moving fast and crates carry `0.0.x` versions:

- **No crates.io publishing yet** — `CARGO_REGISTRY_TOKEN` is *not* set
  in the GitHub repo secrets. The `release-plz` workflow opens Release
  PRs and creates git tags but skips the actual `cargo publish` step.
- Adopters install via git deps. See README "Installing kick-rs".

## When you're ready to ship to crates.io

1. **Reserve the crate names.** Log in to <https://crates.io/me>,
   confirm `kick-rs`, `kick-rs-core`, `kick-rs-http` are still
   available. If any are taken, decide on alternatives *before*
   automating release.
2. **Create a CI token.** crates.io → Account Settings → API Tokens →
   create one scoped to `publish-update`.
3. **Add the secret to GitHub.** Repo Settings → Secrets and variables
   → Actions → New repository secret: `CARGO_REGISTRY_TOKEN`.
4. **Cut the first release.** When the Release PR is next merged the
   workflow's `release-publish` job will run `cargo publish` for each
   releasable crate in dependency order.
5. **(Optional, recommended)** Migrate to crates.io
   [Trusted Publishers](https://blog.rust-lang.org/2025/07/22/crates-io-development-update.html)
   once release-plz supports it — eliminates the long-lived token in
   favor of OIDC short-lived credentials. Drop the secret then.

## Manual fallback — if automation is broken

If release-plz is wedged and you need to ship by hand:

```bash
# 1. Decide the version. Update Cargo.toml for the crate.
# 2. Update its CHANGELOG.md.
# 3. Commit, push, merge to main.

# 4. Publish in dependency order:
cargo publish -p kick-rs-core
cargo publish -p kick-rs-http
cargo publish -p kick-rs

# 5. Tag and push tags:
git tag kick-rs-core-v0.1.0
git tag kick-rs-http-v0.1.0
git tag kick-rs-v0.1.0
git push origin --tags
```

Each `cargo publish` fails fast if the crate isn't ready (e.g., docs
broken, deps unpublished). Fix forward.

## Yanking a bad release

Yanking hides a version from new builds — existing `Cargo.lock` files
keep working. **Versions cannot be deleted.**

```bash
cargo yank --version 0.1.3 kick-rs-core
# To un-yank:
cargo yank --version 0.1.3 kick-rs-core --undo
```

After yanking:
1. Fix the issue.
2. Bump to the next patch (`0.1.4`) and ship via the normal flow. Do
   **not** try to re-publish `0.1.3`.

## Pre-release identifiers (alpha / beta / rc)

For unstable APIs:

```toml
# crates/kick-rs/Cargo.toml
[package]
version = "0.1.0-alpha.1"
```

Cargo treats `0.1.0-alpha.1 < 0.1.0`, so adopters using `kick-rs = "0.1"`
won't pick it up. Useful for opt-in early access:

```toml
# In an adopter's Cargo.toml:
kick-rs = "0.1.0-alpha.1"     # explicit opt-in
```

To leave pre-release mode, the next bump simply drops the suffix
(`0.1.0-alpha.5` → `0.1.0`).

## Git tags

Tags follow `<crate>-v<version>`:

- `kick-rs-core-v0.1.0`
- `kick-rs-http-v0.1.0`
- `kick-rs-v0.1.0`

This avoids ambiguity in a multi-crate workspace where multiple
versions co-exist on the same commit. release-plz manages tag creation
automatically; manual fallback documented above.

## CI gate

Pull requests must pass [`.github/workflows/ci.yml`](./.github/workflows/ci.yml):

- `cargo fmt --all -- --check`
- `cargo build --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`

The `users-api` example is excluded from the workspace and not built
by CI — add it to a matrix job if/when we want that coverage.

## What's not covered yet

- **Trusted Publishers OIDC** — using GitHub Actions OIDC instead of a
  long-lived `CARGO_REGISTRY_TOKEN`. Will adopt once release-plz
  supports it natively.
- **`docs.rs` builds.** docs.rs auto-builds on every crates.io publish
  with `--all-features`. We don't override its config yet; we'll add a
  `[package.metadata.docs.rs]` block once we have features worth
  toggling for docs.
- **Release notes on GitHub.** release-plz can create GitHub Releases
  from CHANGELOG entries. Enable via the workflow's `command:
  release-github` once we're publishing.
- **Backport branches.** No `0.1.x` maintenance branch yet — strict
  trunk-based development. Revisit at v1.0.

---

## Quick reference

| What                 | Where                                       |
|----------------------|---------------------------------------------|
| release-plz config   | [`release-plz.toml`](./release-plz.toml)    |
| Release workflow     | [`.github/workflows/release-plz.yml`](./.github/workflows/release-plz.yml) |
| CI workflow          | [`.github/workflows/ci.yml`](./.github/workflows/ci.yml) |
| Per-crate CHANGELOGs | `crates/<crate>/CHANGELOG.md`               |
| Versioning model     | This doc, section "Versioning model"        |
| Conventional commit reference | <https://www.conventionalcommits.org> |
