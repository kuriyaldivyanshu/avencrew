# Rust output formatter exercise

A standalone, dependency-free coding exercise. `src/format.rs` implements the supplied types; public tests cover bounded byte output, UTF-8 boundaries, exact cursors and invalid input. This crate is not the application harness.

Requires Rust 1.89 or newer. From repository root:

```sh
cargo test --manifest-path fixtures/delegation/q02/Cargo.toml --locked --offline
```

The library forbids unsafe code. No network, database, credentials or other application packages are required.
