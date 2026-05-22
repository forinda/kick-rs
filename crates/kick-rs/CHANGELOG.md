# Changelog

All notable changes to `kick-rs` (the umbrella crate) will be documented
in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Maintained automatically by [release-plz](https://release-plz.dev) from
Conventional Commits. See [`RELEASE.md`](../../RELEASE.md) for the flow.

## [Unreleased]

## [0.0.3](https://github.com/forinda/kick-rs/compare/kick-rs-v0.0.2...kick-rs-v0.0.3) - 2026-05-22

### Added

- *(core)* contributor error matrix via OnErrorAction
- *(http)* ModuleList + ModuleRegistry + Bootstrap::setup for conditional mount
- *(http)* phase-keyword middleware via HttpPlugin::middleware()
- *(plugins)* plugins ship modules+adapters+lifecycle; HttpPlugin ships routes

## [0.0.2](https://github.com/forinda/kick-rs/compare/kick-rs-v0.0.1...kick-rs-v0.0.2) - 2026-05-22

### Added

- *(contributors)* typed ContextContributor pipeline + Ctx<T> extractor

## [0.0.1](https://github.com/forinda/kick-rs/compare/kick-rs-v0.0.0...kick-rs-v0.0.1) - 2026-05-22

### Added

- *(macros)* #[service] proc-macro + ServiceImpl trait
