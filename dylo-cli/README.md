[![license: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![crates.io](https://img.shields.io/crates/v/dylo-runtime.svg)](https://crates.io/crates/dylo-runtime)
[![docs.rs](https://docs.rs/dylo-runtime/badge.svg)](https://docs.rs/dylo-runtime)

# dylo-runtime

`dylo-runtime` generates the consumer crates corresponding to module implementation crates marked with `#[dylo::export]` attributes. This tool scans the workspace for crates starting with `mod-` and generates corresponding `con-` crates that contain just the trait definitions and public interfaces.

## Installation

```
cargo install dylo-runtime
```

Note that dylo-runtime needs `rustfmt` to be present at runtime.

## Usage

The CLI expects to be run from the root of a Cargo workspace containing mod crates. It will:

1. Find all crates prefixed with `mod-`
2. Generate corresponding `con-` crates with trait definitions
3. Add spec files to the original mod crates
4. Verify compilation of generated consumer crates

Basic usage:

```
con
```

Options:
* `--force`: Force regeneration of all consumer crates
* `--mod <NAME>`: Only process the specified mod
* `-h, --help`: Print help information

By default, changes are only made if the source mod crates have been modified more recently than their generated consumer crates.

## con annotations, exporting interfaces etc.

For how to write con-friendly code, see the documentation of the [con crate](https://docs.rs/dylo)
