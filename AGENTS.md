# Proqi engineering contract

These rules apply to all work in this repository. Read `context/PRODUCT.md`
completely before changing visible behavior. Read `context/ARCHITECTURE.md`
completely before changing implementation boundaries or durable contracts.

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
- Before `1.0`, an intentional breaking CLI or JSON change must update the
  current-contract fixtures and the prepared GitHub Release notes.
- Every visible TUI change updates the representative Insta snapshots in the
  same commit. Pending `.snap.new` files fail the canonical quality gate.

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
- Terminal and subprocess workers must have idempotent cancellation and bounded
  teardown. Never add an unbounded thread join or leave a spawned child without
  a panic-safe ownership guard in tests.
- Keyboard and mouse paths receive equivalent behavioral coverage. macOS PTY
  tests cover startup, input, resize, and clean shutdown in CI. Linux retains
  build and terminal-independent integration coverage until the PTY driver is
  portable.

## Design Context

### Users

Proqi serves developers who coordinate several coding agents at once and keep
the board open in a frequently resized terminal pane beside an active agent.
Their job is to capture, edit, organize, recover, and transfer prompts or local
context without interrupting the work already in progress. The interface must
remain comfortable through long working sessions and require almost no setup or
mode-management overhead.

### Brand Personality

Proqi is calm, immediate, and trustworthy. It should create confidence through
precise behavior, truthful persistence state, and predictable interactions. It
stays quiet while the user is thinking and becomes visually assertive only for
focus, an available action, or a state that genuinely requires attention.

### Aesthetic Direction

The interface is minimal, terminal-native, and closer to an unobtrusive Sublime
Text scratchpad than a dashboard or task manager. Use one responsive column,
natural-height titleless thoughts, generous readable whitespace, and a compact
forest-green focus gutter. Separate adjacent thoughts with one quiet horizontal
rule instead of enclosing them in cards. Prefer spacing and contrast over other
borders or permanent chrome. Inherit terminal foreground and background in
automatic mode, support explicit light and dark palettes, and retain a
limited-color fallback.

Forest green is the only routine accent. Other colors communicate real semantic
states only. Do not introduce decorative cards, gradients, glow, texture,
animation, or ornamental brand motifs into the terminal interface. Personality
comes from interaction quality and small precise details, not visual decoration.

### Design Principles

- Disappear until useful. Keep content dominant and reveal controls or emphasis
  only when context makes them relevant.
- Make every interaction direct. Common capture, editing, organization, and
  transfer actions should take one unambiguous step whenever possible.
- Make state unmistakable. Focus, durability, errors, selection, and available
  actions must remain legible without relying on color alone.
- Treat the pane as the design surface. Reflow continuously and preserve content,
  focus, cursor position, scroll, and hit geometry across every supported size.
- Provide equivalent keyboard and mouse workflows. Where Proqi controls the RGB
  palette, target WCAG 2.2 AA contrast. Respect reduced motion, remappable input,
  limited-color terminals, and Unicode text.

## Repository hygiene

- Do not commit secrets, local databases, runtime state, build artifacts,
  temporary review output, or machine-specific paths.
- Do not add another command layer when `xtask` can own the operation.
- The project license is MIT. Do not publish a package, release, tap, public
  artifact, repository setting, tag, or visibility change without Oliver's
  explicit approval of that exact outward action.
