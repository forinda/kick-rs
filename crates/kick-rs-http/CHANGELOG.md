# Changelog

All notable changes to `kick-rs-http` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Maintained automatically by [release-plz](https://release-plz.dev) from
Conventional Commits. See [`RELEASE.md`](../../RELEASE.md) for the flow.

## [Unreleased]

## [0.1.0-alpha.3](https://github.com/forinda/kick-rs/compare/kick-rs-http-v0.1.0-alpha.2...kick-rs-http-v0.1.0-alpha.3) - 2026-05-22

### Added

- *(devtools)* wire Introspect into the /__debug snapshot
- *(http)* AssetsPlugin — serve EmbeddedAssets with cache headers

## [0.1.0-alpha.2](https://github.com/forinda/kick-rs/compare/kick-rs-http-v0.1.0-alpha.1...kick-rs-http-v0.1.0-alpha.2) - 2026-05-22

### Added

- *(http)* DevTools /__debug introspection endpoint
- *(macros,http)* paths!(...) — bulk OpenAPI path registration
- *(http)* auto-collect OpenAPI paths from modules
- *(http)* OpenApiPlugin — serve a utoipa spec at /openapi.json
- *(http)* HelmetPlugin + TraceContextPlugin
- *(http)* built-in plugins — request_id, request_logger, cors, compression

### Other

- release

## [0.1.0-alpha.1](https://github.com/forinda/kick-rs/compare/kick-rs-http-v0.0.3...kick-rs-http-v0.1.0-alpha.1) - 2026-05-22

### Added

- *(macros,http)* route attribute macros (#[get]/#[post]/...) + .handler()

### Other

- [**breaking**] bump to 0.1.0-alpha.1

## [0.0.3](https://github.com/forinda/kick-rs/compare/kick-rs-http-v0.0.2...kick-rs-http-v0.0.3) - 2026-05-22

### Added

- *(http)* ModuleList + ModuleRegistry + Bootstrap::setup for conditional mount
- *(http)* phase-keyword middleware via HttpPlugin::middleware()
- *(examples,http)* multi-tenant example + framework request access
- container access from contributors + #[contributor] proc-macro
- *(plugins)* plugins ship modules+adapters+lifecycle; HttpPlugin ships routes

## [0.0.2](https://github.com/forinda/kick-rs/compare/kick-rs-http-v0.0.1...kick-rs-http-v0.0.2) - 2026-05-22

### Added

- *(contributors)* typed ContextContributor pipeline + Ctx<T> extractor

## [0.0.1](https://github.com/forinda/kick-rs/compare/kick-rs-http-v0.0.0...kick-rs-http-v0.0.1) - 2026-05-22

### Added

- *(macros)* #[service] proc-macro + ServiceImpl trait
