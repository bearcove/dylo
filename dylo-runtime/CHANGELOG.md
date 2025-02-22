# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.5](https://github.com/bearcove/dylo/compare/dylo-runtime-v1.0.4...dylo-runtime-v1.0.5) - 2025-02-22

### Other

- rust 1.85 / edition 2024

## [1.0.4](https://github.com/bearcove/dylo/compare/dylo-runtime-v1.0.3...dylo-runtime-v1.0.4) - 2024-12-06

### Other

- Remove cfg(not(feature = "impl")) attributes

## [1.0.3](https://github.com/bearcove/dylo/compare/dylo-runtime-v1.0.2...dylo-runtime-v1.0.3) - 2024-12-06

### Other

- Add more details when we can't find the sources

## [1.0.2](https://github.com/bearcove/dylo/compare/dylo-runtime-v1.0.1...dylo-runtime-v1.0.2) - 2024-12-06

### Other

- I suspect the presence of slashes gave us linker errors...

## [1.0.1](https://github.com/bearcove/dylo/compare/dylo-runtime-v1.0.0...dylo-runtime-v1.0.1) - 2024-12-05

### Other

- Compat check
- Yeah ok we need to expose rubicon features through dylo-runtime, a non-proc-macro crate
- Change CON_ to DYLO_
- Don't be so chatty
- Default to finding everything, not necessarily under a mods/ folder
