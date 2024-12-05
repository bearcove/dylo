[![license: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)
[![crates.io](https://img.shields.io/crates/v/con.svg)](https://crates.io/crates/con)
[![docs.rs](https://docs.rs/con/badge.svg)](https://docs.rs/con)

# con

`con` generates "consumer" versions of your "module" crates.

"modules" are [cdylibs](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#library) library crates that only expose "dyn compatible" traits.

"consumers" are regular (rlib) library crates that only contain the trait definitions and data structures used in the public API of the corresponding module.

This pattern is doable by hand:

  * Put all your `pub struct` and `pub trait` in some "interface" crate
  * Create a separate "impl" crate that implements all those traits
  * Have the interface crate know how to build and load the impl as a dynamic library
  * Make sure that the "impl" crate and "interface" crate are actually ABI-compatible,
    (something [rubicon](https://github.com/bearcove/rubicon) can help with)

But it's a lot of going back and forth and adapting the impl to the trait or the traits to the impl.

A block like this contains everything you need to declare the corresponding trait:

```rust
#[con::export]
impl RequestBuilder for RequestBuilderImpl {
    fn add_header(&mut self, name: &str, value: &str) -> &mut Self {
        self.headers.push((name.to_string(), value.to_string()));
    }

    fn with_header(mut self: Box<Self>, name: &str, value: &str) -> Box<Self> {
        self.add_header(name, value);
        self
    }
}
```

And that's exactly what `con` relies on: it generates a trait definition like this:

```rust
pub trait RequestBuilder: Sync + Send + 'static {
    fn add_header(&mut self, name: &str, value: &str) -> &mut Self;
    fn with_header(self: Box<Self>, name: &str, value: &str) -> Box<Self>;
}
```

However, that trait needs to live in both versions of the crate: the `mod-markdown` version
(the actual implementation) and the `con-markdown` version.

A proc macro would be able to add the trait definition to the `mod-markdown` module, but
it would slow down its build time significantly, bringing in all sorts of dependencies like
[syn](https://crates.io/crates/syn).

So instead, although `con` itself _is_ a proc-macro crate, it has zero dependencies, and
doesn't transform the token stream at all.

It merely defines attributes like `[con::export]`, that a separate tool, `con-cli`, will look for,
to know which trait definitions to generate.

For more information, read crate-level documentations for:

  * [con-cli](https://crates.io/crates/con-cli)
  * [con](https://crates.io/crates/con)
  * [con-loader](https://crates.io/crates/con-loader)
