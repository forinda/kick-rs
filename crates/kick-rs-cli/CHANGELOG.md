# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.3](https://github.com/forinda/kick-rs/compare/kick-rs-cli-v0.1.0-alpha.2...kick-rs-cli-v0.1.0-alpha.3) - 2026-05-22

### Added

- *(cli)* cargo kick info — print a snapshot of the current project
- *(cli)* cargo kick add <feature> — toggle umbrella features in Cargo.toml

## [0.1.0-alpha.2](https://github.com/forinda/kick-rs/compare/kick-rs-cli-v0.1.0-alpha.1...kick-rs-cli-v0.1.0-alpha.2) - 2026-05-22

### Added

- *(cli)* auto-register generated code into main.rs / module mod.rs
- *(cli)* cargo kick g contributor <module>/<name>

### Other

- release

## [0.1.0-alpha.1](https://github.com/forinda/kick-rs/releases/tag/kick-rs-cli-v0.1.0-alpha.1) - 2026-05-22

### Added

- *(cli)* cargo kick g service <module>/<name>
- *(cli)* cargo kick g module <name> — codegen into existing project
- *(cli)* cargo kick new — scaffold a fresh kick-rs project

### Other

- [**breaking**] rename project to kick-rs (umbrella + all sub-crates)
