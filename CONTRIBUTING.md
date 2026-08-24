# Contributing to Proqi

Thank you for helping improve Proqi. Issues and pull requests are the public
collaboration channels for this project.

## Before you begin

Proqi requires Git, Rust 1.88 or newer, and the tools checked by:

```shell
cargo xtask setup
```

The repository pins its normal development compiler in `rust-toolchain.toml`.
Rust 1.88 is the minimum supported version during the `0.x` series. Clone the
repository, enter its directory, and verify the baseline before editing:

```shell
git clone https://github.com/oborchers/proqi.git
cd proqi
cargo xtask check
```

`cargo xtask setup` reports missing tools. It does not modify global state.
The optional checked-in pre-commit hook is enabled explicitly with:

```shell
cargo xtask install-hooks
```

CI remains authoritative even when the local hook is installed.

## Choose the right scope

Bug fixes and focused improvements are welcome. Please open an Issue before a
change that alters product scope, architecture boundaries, durable storage,
public behavior, release policy, or a major dependency. This lets maintainers
confirm the problem and intended outcome before substantial work begins.

Security vulnerabilities must not be reported in a public Issue or pull
request. Follow [SECURITY.md](SECURITY.md).

## Architecture

Read [context/PRODUCT.md](context/PRODUCT.md) before changing visible behavior
and [context/ARCHITECTURE.md](context/ARCHITECTURE.md) before changing
implementation boundaries or durable contracts.

Dependencies point inward:

```text
domain <- ports <- application <- adapters and UI composition
```

Domain code owns entities and invariants. Ports describe terminal-independent
capabilities. Application code coordinates domain values through ports.
Adapters translate SQLite, terminal, clipboard, filesystem, process, and Herdr
behavior. The UI translates input into application actions and renders state.
Use `cargo xtask architecture` to verify these boundaries.

Keep behavior deterministic. Inject clocks, identifiers, paths, environment,
filesystem, clipboard, and process execution where tests must control them.
Do not make tests depend on order, user state, or arbitrary wall-clock sleeps.

## Implement and test

Add focused tests with each behavior change. Bug fixes should include a
regression test where practical. Important failure paths need explicit tests,
especially persistence, concurrency, Unicode, terminal restoration, clipboard,
and external process behavior.

Useful focused commands include:

```shell
cargo test --test editor_contract
cargo test --test sqlite_store
cargo test --test ui_board
cargo test --test cli_workflow
cargo xtask test-pty
```

Use `cargo test -- --list` or `cargo nextest list` to discover current tests.
Run the smallest relevant test while iterating, then run the canonical gate:

```shell
cargo xtask check
```

Milestone and release work also uses:

```shell
cargo xtask audit
cargo xtask package
```

## Code guardrails

- Format Rust with the checked-in rustfmt configuration.
- Treat every Clippy warning as an error.
- Keep every first-party source file at or below 500 physical lines.
- Keep Rust functions within the checked-in function-length, cognitive
  complexity, and nesting thresholds.
- Keep production code free of `unwrap`, `expect`, `panic!`, `unreachable!`,
  `todo!`, and `unimplemented!`.
- Keep unsafe Rust absent unless a reviewed architecture decision proves it is
  unavoidable.
- Preserve complete typed UUIDv7 identifiers at every durable and external
  boundary.
- Do not commit secrets, local databases, runtime state, build artifacts,
  machine-specific paths, or temporary review output.

## Snapshots and golden files

User-visible TUI changes require representative Insta snapshots in the same
commit. Run the focused snapshot tests, inspect every changed cell and style,
and review `.snap` diffs before committing. Never auto-accept snapshots. A
pending `.snap.new` file fails `cargo xtask check`.

The same review rule applies to other generated or golden artifacts. Generation
commands must be reproducible, but updates always require human diff review.

## Commits and pull requests

Keep commits focused, buildable, and written with an imperative subject. Do not
mix unrelated cleanup into a behavior change. Before opening a pull request:

1. Rebase or merge the current `main` according to the repository's current
   contribution guidance.
2. Run focused tests for the changed behavior.
3. Run `cargo xtask check`.
4. Update product, architecture, user, skill, or maintainer documentation when
   its contract changed.
5. Inspect the complete diff and every snapshot.

The pull request should explain the user-visible outcome, important design
choices, failure behavior, and exact verification performed. Link the relevant
Issue when one exists. Do not claim a platform or test result that was not run.

### Pull request checklist

- [ ] The change has one clear purpose.
- [ ] Focused tests cover behavior and important failure paths.
- [ ] `cargo xtask check` passes.
- [ ] Architecture and source-size rules pass.
- [ ] Snapshot and golden-file changes were reviewed explicitly.
- [ ] Public documentation and the Proqi skill remain accurate.
- [ ] No secret, private content, runtime file, or machine path is included.
- [ ] Skipped or platform-only verification is reported plainly.

## License terms

Proqi is licensed under the [MIT License](LICENSE). Contributions are accepted
under the same MIT terms, using the standard inbound-equals-outbound model. No
contributor license agreement or Developer Certificate of Origin sign-off is
required.
