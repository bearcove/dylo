[![license: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![crates.io](https://img.shields.io/crates/v/dylo-cli.svg)](https://crates.io/crates/dylo-cli)
[![docs.rs](https://docs.rs/dylo-cli/badge.svg)](https://docs.rs/dylo-cli)

# dylo-cli

`dylo-cli` generates the consumer crates corresponding to module implementation crates marked with `#[dylo::export]` attributes. This tool scans the workspace for crates starting with `mod-` and generates corresponding consumer crates that contain just the trait definitions and public interfaces.

## Installation

```
cargo install dylo-cli
```

## Usage

The CLI expects to be run from the root of a Cargo workspace containing mod crates. It will:

1. Find all crates prefixed with `mod-`
2. Generate corresponding consumer crates with trait definitions
3. Add spec files to the original mod crates
4. Verify compilation of generated consumer crates

Basic usage:

```
dylo gen
```

Options:
* `--force`: Force regeneration of all consumer crates
* `--mod <NAME>`: Only process the specified mod
* `-h, --help`: Print help information

By default, changes are only made if the source mod crates have been modified more recently than their generated consumer crates.

## dylo annotations, exporting interfaces etc.

For how to write dylo-friendly code, see the documentation of the [dylo crate](https://docs.rs/dylo)
