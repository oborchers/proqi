# Proqi

Proqi is a terminal-native scratchpad for people who work with several coding
agents at once. Each process owns one resumable board of independently editable
thoughts. The project is currently a private alpha under active development.

## Prerequisites

Install Rust through `rustup`. The checked-in toolchain file selects the exact
compiler and required components. Developer quality commands additionally use
`cargo-nextest`, `cargo-llvm-cov`, `cargo-deny`, and `cargo-audit`.

Verify the local setup:

```shell
cargo xtask setup
```

## Build and run

```shell
cargo build --locked
cargo run --bin proqi
```

The current scaffold exposes help and version information. The interactive
board is implemented incrementally against `PRODUCT.md` and `ARCHITECTURE.md`.

## Development

```shell
cargo xtask format       # apply formatting
cargo xtask check        # formatting, Clippy, tests, and documentation tests
cargo xtask test         # deterministic test suite
cargo xtask test-pty     # pseudo-terminal scenarios
cargo xtask coverage     # write target/coverage/lcov.info
cargo xtask audit        # advisories, licenses, sources, and dependency policy
cargo xtask package      # release build plus temporary-prefix launch smoke test
```

CI invokes the same `xtask` commands. Run `cargo xtask check` before committing.

## Product and architecture

`PRODUCT.md` is the source of truth for user-visible behavior.
`ARCHITECTURE.md` defines the implementation boundaries and quality contract.

The final open-source license, Homebrew package identity, signing, notarization,
and publication setup remain deliberately undecided. Nothing in this private
version publishes artifacts or changes repository visibility.
