# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [4.4.0](https://github.com/bearcove/dylo/compare/dylo-cli-v3.0.1...dylo-cli-v4.4.0) - 2025-03-22

### Added

- [**breaking**] Disable dylo's building functionality, have it look in `../lib`

## [3.0.1](https://github.com/bearcove/dylo/compare/dylo-cli-v3.0.0...dylo-cli-v3.0.1) - 2025-03-21

### Other

- Add 'dylo list' subcommand

## [3.0.0](https://github.com/bearcove/dylo/compare/dylo-cli-v2.2.0...dylo-cli-v3.0.0) - 2025-03-04

### Added

- [**breaking**] Improve scope control for code generation
- Allow running in a subdir

## [2.2.0](https://github.com/bearcove/dylo/compare/dylo-cli-v2.1.0...dylo-cli-v2.2.0) - 2025-03-04

### Added

- Actually prepend allow unused imports to generate consumer module

## [2.1.0](https://github.com/bearcove/dylo/compare/dylo-cli-v2.0.1...dylo-cli-v2.1.0) - 2025-03-04

### Added

- *(dylo-cli)* add generated code notice and unused_imports allowance

## [2.0.1](https://github.com/bearcove/dylo/compare/dylo-cli-v2.0.0...dylo-cli-v2.0.1) - 2025-03-04

### Other

- Migrate to clap for command line argument parsing

## [2.0.0](https://github.com/bearcove/dylo/compare/dylo-cli-v1.0.7...dylo-cli-v2.0.0) - 2025-03-04

### Added

- *(dylo-cli)* [**breaking**] implement add/remove dependency commands

### Other

- Remove unnecessary deps
- more debugging

## [1.0.7](https://github.com/bearcove/dylo/compare/dylo-cli-v1.0.6...dylo-cli-v1.0.7) - 2025-03-01

### Other

- Only print updates if changes were made

## [1.0.6](https://github.com/bearcove/dylo/compare/dylo-cli-v1.0.5...dylo-cli-v1.0.6) - 2025-02-28

### Other

- collect removed deps and show single info message

## [1.0.5](https://github.com/bearcove/dylo/compare/dylo-cli-v1.0.4...dylo-cli-v1.0.5) - 2025-02-22

### Other

- rust 1.85 / edition 2024

## [1.0.4](https://github.com/bearcove/dylo/compare/dylo-cli-v1.0.3...dylo-cli-v1.0.4) - 2024-12-06

### Other

- Use prettyplease rather than rustfmt
- Remove cfg(not(feature = "impl")) attributes

## [1.0.3](https://github.com/bearcove/dylo/compare/dylo-cli-v1.0.2...dylo-cli-v1.0.3) - 2024-12-06

### Other

- I suspect the presence of slashes gave us linker errors...

## [1.0.2](https://github.com/bearcove/dylo/compare/dylo-cli-v1.0.1...dylo-cli-v1.0.2) - 2024-12-05

### Other

- Avoid 'dylo runtime is unused' warnings
- Yeah ok we need to expose rubicon features through dylo-runtime, a non-proc-macro crate
- Don't be so chatty
- Default to finding everything, not necessarily under a mods/ folder
- Suffix include! items
- Add dylo-runtime, keep optional deps if they're enabled by other features

## [1.0.1](https://github.com/bearcove/dylo/compare/dylo-cli-v1.0.0...dylo-cli-v1.0.1) - 2024-12-05

### Other

- Remove cfg_attr(feature = 'impl', etc.) attributes
