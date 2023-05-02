<h1 align="center">cargo-px</h1>
<div align="center">
 <strong>
   Cargo Power eXtensions
 </strong>
</div>

<br />

<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/cargo-px">
    <img src="https://img.shields.io/crates/v/cargo-px.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/cargo-px">
    <img src="https://img.shields.io/crates/d/cargo-px.svg?style=flat-square"
      alt="Download" />
  </a>
</div>
<br/>

Check out the [announcement post](https://lpalmieri.com/posts/cargo-px) to learn more about `cargo-px` and the problems it solves with respect to code generation in Rust projects.

# Table of Contents
0. [How to install](#how-to-install)
1. [How to use](#how-to-use)
2. [Verify that the generated code is up-to-date](#verify-that-the-generated-code-is-up-to-date)
3. [License](#license)

## How To Install 

You can install `cargo-px` from [crates.io](https://crates.io) with

```bash
cargo install cargo-px --locked
```

## How to use

It is designed as a **`cargo` proxy**: instead of invoking `cargo <CMD> <ARGS>`, you go for `cargo px <CMD> <ARGS>`. For example, you go for `cargo px build --all-features` instead of `cargo build --all-features`.

`cargo px` examines your workspace every time you invoke it.  
If any of your crates needs to be generated, it will invoke the respective code generators before forwarding the command and its arguments to cargo.

`cargo px` leverages the [`metadata` section](https://doc.rust-lang.org/cargo/reference/manifest.html#the-metadata-table).  
In the crate that you want to see generated, you fill in the [`package.metadata.px.generate`] section as follows: 

```toml
[package]
name = "..."
version = "..."
# [...]

[package.metadata.px.generate]
# The generator is a binary in the current workspace. 
# It's the only generator type we support at the moment.
generator_type = "cargo_workspace_binary"
# The name of the binary.
generator_name = "bp"
# The arguments to be passed to the binary. 
# It can be omitted if there are no arguments.
generator_args = ["--quiet", "--profile", "optimised"]
```

`cargo-px` will detect the configuration and invoke `cargo run --bin bp -- --quiet --profile="optimised"` for you.  
If there are multiple crates that need to be code-generated, `cargo-px` will invoke the respective code-generators in an order that takes into account the dependency graph (i.e. dependencies are always code-generated before their dependents).

`cargo-px` will also set two environment variables for the code generator:

- `CARGO_PX_GENERATED_PKG_MANIFEST_PATH`, the path to the `Cargo.toml` file of the crate that needs to be generated;
- `CARGO_PX_WORKSPACE_ROOT_DIR`, the path to the `Cargo.toml` file that defines the current workspace (i.e. the one that contains the `[workspace]` section).

## Verify that the generated code is up-to-date

If you are committing the generated code, it might be desirable to verify in CI that it's up-to-date.  
You can do so by running:

```bash
# Triggers code-generation and verifies that the code compiles
cargo px check
# Returns an error if the code generation step created new files or 
# modified any of the files tracked by `git` in version control
git diff --quiet HEAD || (echo "The code-generated crates are stale. Re-run 'cargo px check locally and commit the results" && exit 1)
```

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
