# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
