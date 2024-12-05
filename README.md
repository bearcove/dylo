[![license: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![crates.io](https://img.shields.io/crates/v/dylo.svg)](https://crates.io/crates/dylo)
[![docs.rs](https://docs.rs/dylo/badge.svg)](https://docs.rs/dylo)

# dylo

`dylo` takes a "module" crate and generates a "consumer" crate that knows how to build
and load that original crate — and that exposes the exact same public API.

"modules" are [cdylibs](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#library) library crates that only expose "dyn compatible" traits.

"consumers" are rlib library crates (the default) that usually have fewer dependency, and
only export set of dyn-compatible traits, along with the struct/enum types used in the public API.

dylo relies on code generation: the [dylo-cli](https://crates.io/crates/dylo-cli) tool looks
for annotations from the [dylo](https://crates.io/crates/dylo) proc-macro crate, to know which
"impl Trait for TraitImpl" blocks should be used to generate public traits.

Although that pattern is doable by hand, dylo takes a lot of the human error and repetitive work
out of the equation.

To learn more, read the various crate documentation in order:

  * [dylo](https://crates.io/crates/dylo)
  * [dylo-cli](https://crates.io/crates/dylo-cli)
  * [dylo-runtime](https://crates.io/crates/dylo-runtime)
