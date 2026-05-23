# Changelog

All notable changes to `kick-rs` (the umbrella crate) will be documented
in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Maintained automatically by [release-plz](https://release-plz.dev) from
Conventional Commits. See [`RELEASE.md`](../../RELEASE.md) for the flow.

## [Unreleased]

## [0.1.1](https://github.com/forinda/kick-rs/compare/kick-rs-v0.1.0...kick-rs-v0.1.1) - 2026-05-23

### Other

- release

## [0.1.0](https://github.com/forinda/kick-rs/compare/kick-rs-v0.1.0-alpha.6...kick-rs-v0.1.0) - 2026-05-23

### Added

- *(examples)* bundle static assets into users-api via AssetsPlugin

### Other

- *(release)* graduate all crates from 0.1.0-alpha.X to 0.1.0

## [0.1.0-alpha.6](https://github.com/forinda/kick-rs/compare/kick-rs-v0.1.0-alpha.5...kick-rs-v0.1.0-alpha.6) - 2026-05-23

### Other

- include each crate's README as its docs.rs landing page

## [0.1.0-alpha.5](https://github.com/forinda/kick-rs/compare/kick-rs-v0.1.0-alpha.4...kick-rs-v0.1.0-alpha.5) - 2026-05-22

### Other

- updated the following local packages: kick-rs-core, kick-rs-assets, kick-rs-http, kick-rs-config, kick-rs-macros

## [0.1.0-alpha.4](https://github.com/forinda/kick-rs/compare/kick-rs-v0.1.0-alpha.3...kick-rs-v0.1.0-alpha.4) - 2026-05-22

### Added

- *(assets)* real AssetManifest + embed_assets! macro

## [0.1.0-alpha.3](https://github.com/forinda/kick-rs/compare/kick-rs-v0.1.0-alpha.2...kick-rs-v0.1.0-alpha.3) - 2026-05-22

### Other

- updated the following local packages: kick-rs-config

## [0.1.0-alpha.2](https://github.com/forinda/kick-rs/compare/kick-rs-v0.1.0-alpha.1...kick-rs-v0.1.0-alpha.2) - 2026-05-22

### Added

- *(http)* DevTools /__debug introspection endpoint
- *(macros,http)* paths!(...) — bulk OpenAPI path registration
- *(config)* real layered Config loader (defaults + file + env)

### Other

- *(examples)* wire OpenAPI through users-api via paths!()
- release

## [0.1.0-alpha.1](https://github.com/forinda/kick-rs/compare/kick-rs-v0.0.3...kick-rs-v0.1.0-alpha.1) - 2026-05-22

### Other

- [**breaking**] bump to 0.1.0-alpha.1

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
