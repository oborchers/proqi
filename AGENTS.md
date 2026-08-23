# Proqi engineering contract

These rules apply to all work in this repository. Read `PRODUCT.md` completely
before changing visible behavior. Read `ARCHITECTURE.md` completely before
changing implementation boundaries or durable contracts.

`AGENTS.md` is the canonical instruction file at every scope. Each one must have
a sibling relative `CLAUDE.md` symlink that points to it. Never duplicate or
independently edit instruction content in `CLAUDE.md`.

## Verification

- `cargo xtask check` is the canonical local quality gate.
- Run focused tests while developing, then the complete gate before committing.
- Behavior changes require tests that prove the behavior and important failure
  paths. Bug fixes require a regression test where practical.
- Use `cargo xtask audit` and `cargo xtask package` at milestone and release
  gates. Coverage is a floor, not evidence that critical invariants are correct.
- Never weaken a gate, threshold, or test merely to make a change pass.
- Never auto-accept snapshots or golden files. Review their diffs explicitly.

## Architecture

Dependencies point inward:

```text
domain <- ports <- application <- adapters and UI composition
```

- Domain owns entities, typed identifiers, value types, and invariants. It does
  not import application, ports, adapters, terminal, SQL, process, filesystem,
  environment, or clipboard implementations.
- Ports describe terminal-independent capabilities in domain terms.
- Application depends on domain values and ports, never concrete adapters or UI.
- Adapters translate external systems into ports. UI translates input into
  application actions and renders application state.
- SQLite belongs below `src/adapters/sqlite`, Crossterm below the terminal
  adapter, Ratatui below UI, and child process execution below a process adapter.
- Time, IDs, paths, environment, filesystem, clipboard, and child processes use
  injected ports where behavior must be deterministic or platform independent.
- Keep one canonical import path for each public type. Implementation modules
  stay private, and adapter internals prefer `pub(crate)` or `pub(super)`.
- Constructors enforce invariants. Keep fields private when direct mutation
  could produce invalid state.

## Rust guardrails

- Unsafe Rust is forbidden unless an explicit architecture decision documents
  and reviews an unavoidable need.
- Production code does not use `unwrap`, `expect`, `panic!`, `unreachable!`,
  `todo!`, or `unimplemented!`. Return typed errors. A proven invariant may use
  a narrow
  `#[expect(..., reason = "...")]` instead of a broad allow.
- Every first-party source file is at most 500 physical lines.
- Rust functions are limited by the checked-in Clippy cognitive-complexity,
  function-length, and nesting thresholds.
- Keep tests deterministic. Inject clocks, identifiers, paths, and process
  execution. Do not depend on test order, wall-clock timing, or user state.
- Preserve complete prefixed UUIDv7 values at every boundary. Do not accept
  durable identifiers as unvalidated generic strings.

## Terminal interface

These rules become mandatory with the first real TUI implementation:

- Rendering remains deterministic and testable with an in-memory backend.
- Crossterm input becomes normalized application actions before the reducer.
- Resize and wrapping preserve content, logical cursor, selection, focus, and
  valid scroll bounds across narrow, wide, tall, and shallow viewports.
- Unicode tests cover wide characters, combining marks, emoji sequences, tabs,
  controls, and wrapped selections.
- Terminal setup uses an RAII guard. Tests prove restoration after normal exit,
  errors, panics, and supported termination signals.
- Keyboard and mouse paths receive equivalent behavioral coverage. macOS PTY
  tests cover startup, input, resize, and clean shutdown in CI. Linux and
  Windows retain build and terminal-independent integration coverage until the
  PTY driver is portable.

## Repository hygiene

- Do not commit secrets, local databases, runtime state, build artifacts,
  temporary review output, or machine-specific paths.
- Do not add another command layer when `xtask` can own the operation.
- Do not select or publish a license, package, release, tap, or public artifact
  until the corresponding product decision is made explicitly.
