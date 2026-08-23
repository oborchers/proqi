# Architecture

Status: Initial architecture decision record

Project: Proqi

Command: `proqi`
Last updated: 2026-08-23

## Purpose

This document defines the technical shape of the terminal-native scratchpad
described in the product vision. It records the boundaries that should remain
stable while individual libraries and implementation details evolve.

The application is a local, resumable board of independently editable thoughts.
Several instances may run at once, normally beside several coding-agent panes.
Each instance owns one session. All sessions share one local database, but two
processes may never edit the same session concurrently.

The architecture optimizes for five properties:

- Text capture and editing must remain immediate during rapid terminal resizing.
- Committed content and undo history must survive process and machine failure.
- Multiple application versions may temporarily run after a package-manager
  update without corrupting shared state.
- Clipboard use works everywhere, while adjacent-agent submission remains an
  optional, capability-gated enhancement.
- The core remains testable without a real terminal, clipboard, database, or
  Herdr process.

## Architectural style

The first implementation is a modular monolith in one Rust workspace. It is not
a collection of services and does not require a background daemon.

The dependency direction is inward:

```text
Terminal events       CLI commands       Local control requests
       |                    |                       |
       +--------------------+-----------------------+
                            |
                    Application services
                    state, commands, effects
                            |
                    Domain model and rules
                            |
          +-----------------+------------------+
          |                 |                  |
      Storage port     Clipboard port     Agent port
          |                 |                  |
       SQLite        native / OSC 52       Herdr CLI
```

Domain and application modules know traits, value types, and errors. They do
not know Ratatui widgets, Crossterm events, SQL statements, subprocess syntax,
or operating-system clipboard APIs. Adapters depend on these inner modules.

This gives the product clean seams without committing to a plugin system or a
large multi-crate abstraction hierarchy before one is needed.

## Technology decisions

### Language and build

- Rust using the current stable toolchain and the Rust 2024 edition.
- A checked-in `rust-toolchain.toml` defines the supported compiler version.
- The minimum supported Rust version follows the highest minimum required by a
  direct dependency. It is tested in CI rather than merely documented.
- Cargo owns dependency resolution. `cargo-dist` or an equivalent release tool
  produces platform archives, checksums, and package-manager metadata.

Rust provides a single native executable, predictable resource use, strong
cross-platform support, and a mature terminal ecosystem. It also makes it
possible to ship without a Node, Python, or JVM runtime.

### Terminal interface

- Ratatui owns terminal-independent drawing primitives and widget composition.
- Crossterm owns terminal setup, input, resize, mouse, bracketed-paste, and
  capability handling.
- `unicode-segmentation` and `unicode-width` define grapheme and terminal-cell
  behavior. Byte indices are never treated as visual columns.
- The editor port is backed by Ropey. A dependency spike against
  `ratatui-textarea` 0.9.2 confirmed useful Unicode wrapping and selection
  behavior, but the crate normalizes CRLF input and keeps wrapped mouse mapping
  behind private state. Those constraints conflict with exact paste preservation
  and layout-derived mouse hit testing. Ratatui therefore renders Proqi's own
  editor snapshot rather than owning the text model.

There is no web view and no browser runtime. Playwright is therefore not the
primary end-to-end tool. Terminal behavior is tested through pseudo-terminal
processes and deterministic render snapshots.

### Persistence

- SQLite through `rusqlite` with SQLite bundled into the release binary.
- WAL journal mode for concurrent readers and short writes from independent
  sessions.
- `synchronous=FULL` initially, because the product promises that acknowledged
  commits survive an operating-system crash or power loss.
- A bounded busy timeout and bounded retry with jitter for transient writer
  contention.
- Native platform data directories resolved through a path facade.

SQLite is the correct default because the data is local, relational, searchable,
transactional, and shared by several processes. A file per thought would make
atomic reorder, persistent undo, full-text search, and concurrent mutation much
harder. A client-server database would add installation and operational cost
without adding product value.

### Supporting libraries

- `clap` for interactive launch flags and scriptable subcommands.
- `serde` with TOML for configuration and JSON for stable machine output.
- `arboard` for the native clipboard, with OSC 52 behind the same facade.
- `tracing` for diagnostics with content redaction by default.
- UUID version 7 identifiers for opaque, sortable entity IDs. Public IDs use a
  typed resource prefix plus 26 characters of canonical lowercase, unpadded
  base32hex. The encoding preserves all 128 UUID bits, is URL safe, and retains
  byte ordering in lexical form. SQLite stores the same UUID as a 16-byte BLOB.
  Prefixes are `ses` for sessions, `tht` for thoughts, `rev` for revisions,
  `op` for durable operations, `ins` for running instances, `req` for
  idempotent control requests, and `sub` for Proqi submission receipts.
- A cross-platform advisory file-lock library for session and schema locks.
- Bounded channels for event and effect communication. An async runtime is not
  introduced until a real concurrent I/O requirement justifies it.

## Runtime model

### One owner of mutable UI state

The main thread owns `AppState`. No worker mutates it directly. Every input is
translated into a domain-level `Action`, and one reducer determines the next
state plus any required effects:

```text
(AppState, Action) -> (AppState, Vec<Effect>)
```

Actions include key and mouse intentions, paste payloads, resize events,
storage acknowledgements, clipboard results, integration results, and timer
ticks. They do not contain Crossterm-specific key codes after translation.

Effects include durable writes, clipboard access, Herdr discovery and
submission, filesystem work, and other operations that may block. Effect
results return as new actions. This keeps rendering and input responsive while
the database or a subprocess is slow.

### Execution lanes

The initial process has four logical lanes:

1. The UI lane reduces actions, computes layout, and renders the latest state.
2. The input lane reads terminal events and sends normalized input messages.
3. The storage lane owns one SQLite connection and executes ordered persistence
   commands.
4. The external-effects lane handles clipboard and integration calls with
   explicit timeouts.

Channels are bounded. Resize events may be coalesced to the newest dimensions.
Text edits and structural operations may not be dropped or reordered.

### Autosave acknowledgement

The UI applies edits optimistically and marks their latest operation sequence as
pending. The storage lane commits short batches in order. Only a successful
transaction advances the durable sequence displayed by the application.

Typing edits may be coalesced into semantic revisions over a short interval.
Paste, create, cut, delete, reorder, submit-and-remove, focus loss, and normal
exit are immediate commit boundaries. Normal exit flushes pending operations.
A process killed before an operation is acknowledged may lose only that pending
operation, never an operation already presented as durable.

On a storage error, the in-memory buffer remains available. The interface shows
that it is not durable and offers retry or export. It never silently changes a
failed save into a successful one.

## Core facades

Facades are internal Rust traits. They are narrow contracts, not public plugin
APIs. Methods below are representative and may use richer typed arguments.

### `SessionService`

This is the application facade used by both the TUI and the CLI.

Responsibilities:

- Start, continue, resume, search, rename, and delete sessions.
- Acquire a session lease before returning an editable session.
- Create, update, move, copy, cut, delete, restore, and search thoughts.
- Coordinate persistent editor and board undo.
- Enforce command preconditions and return structured application errors.
- Produce read models suited to the board and session browser.

It orchestrates ports but contains no terminal, SQL, or Herdr code.

### `Store`

The storage facade exposes transactions in domain terms:

```rust
trait Store {
    fn load_session(&mut self, id: SessionId) -> Result<SessionSnapshot>;
    fn search_sessions(&mut self, query: SessionQuery) -> Result<Vec<SessionHit>>;
    fn commit(&mut self, batch: OperationBatch) -> Result<CommitReceipt>;
    fn undo(&mut self, session: SessionId, scope: UndoScope) -> Result<CommitReceipt>;
    fn redo(&mut self, session: SessionId, scope: UndoScope) -> Result<CommitReceipt>;
}
```

Callers never issue SQL, manage WAL files, or interpret SQLite error codes.
The adapter maps lock, busy, corruption, disk-full, and incompatible-schema
conditions into explicit storage errors.

### `Editor`

One editor instance owns the transient editing state for one focused thought.

```rust
trait Editor {
    fn apply(&mut self, command: EditCommand) -> EditOutcome;
    fn set_viewport(&mut self, viewport: TextViewport);
    fn snapshot(&self) -> EditorSnapshot;
    fn replace_content(&mut self, text: String, cursor: TextPosition);
}
```

`EditCommand` covers insertion, deletion, movement, selection, paste, undo,
redo, and mouse placement. `TextPosition` is logical and survives wrapping and
resize. The facade prevents a library-specific cursor model from leaking into
the application.

### `LayoutEngine`

Layout is a pure function of board state, editor state, terminal capabilities,
and viewport dimensions. It returns a `LayoutSnapshot` containing rectangles,
wrapped visual lines, scroll bounds, focus geometry, and mouse hit targets.

The renderer consumes this snapshot. Mouse handling consults the same snapshot.
This prevents visual geometry and clickable geometry from drifting apart after
a resize.

### `Clipboard`

```rust
trait Clipboard {
    fn read_text(&mut self) -> Result<String, ClipboardError>;
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}
```

Native clipboard and OSC 52 are adapter choices. Cut is an application
transaction: write the exact content first, then commit deletion. Clipboard
failure leaves the thought unchanged.

### `AgentGateway`

```rust
trait AgentGateway {
    fn capabilities(&mut self) -> Result<AgentCapabilities>;
    fn adjacent_targets(&mut self, context: PaneContext) -> Result<Vec<AgentTarget>>;
    fn submit(&mut self, request: SubmissionRequest) -> Result<SubmissionReceipt>;
}
```

The gateway returns verified targets and typed readiness states. It never
returns a convenient but unverified pane. Submission success means the harness
accepted the prompt operation. It does not mean the agent finished processing
the prompt.

### `RuntimeCoordinator`

This facade owns session exclusion, active-instance metadata, schema exclusion,
and update compatibility. Its locks are independent from SQLite transactions.

```rust
trait RuntimeCoordinator {
    fn acquire_session(&mut self, session: SessionId) -> Result<SessionLease>;
    fn acquire_schema_shared(&mut self) -> Result<SchemaLease>;
    fn try_acquire_schema_exclusive(&mut self) -> Result<MigrationLease>;
    fn active_instances(&self) -> Result<Vec<InstanceInfo>>;
}
```

The operating-system lock is authoritative. JSON or database metadata exists
only to explain which process, version, session, and launch directory holds a
lock. Process exit releases the authoritative lock even after a crash.

### `TerminalSession`

This facade enters and restores terminal modes through an RAII guard. Setup and
teardown cover raw mode, alternate screen, bracketed paste, mouse capture,
cursor state, and panic restoration. Restoration is attempted on normal exit,
error return, panic, and termination signals supported by the platform.

### Small deterministic ports

`Clock`, `IdGenerator`, `Paths`, and `ProcessRunner` are injected wherever
wall-clock time, identifiers, platform directories, or subprocess execution
would otherwise make tests nondeterministic.

## Domain model and database

The database stores current state and operation history. It is not a pure
event-sourced system.

### Principal records

- `sessions`: identity, optional name, original and last-opened directories,
  timestamps, last durable operation sequence, and deletion state.
- `thoughts`: session, current content, integer position, timestamps, collapse
  preference, and deletion state.
- `thought_revisions`: coalesced text revisions with enough data to restore the
  previous and next content and cursor state.
- `operations`: ordered structural operations and their inverse payloads for
  persistent undo and redo.
- `integration_context`: optional last-known terminal and verified agent
  metadata. Pane IDs are hints, never durable identity.
- `schema_meta`: schema version, migration history, and application storage
  protocol version.

Full-text search indexes session names, paths, and current thought content.
Search indexes are derived and rebuildable. User content remains canonical in
ordinary tables.

### Invariants

- Every thought belongs to exactly one session.
- Thought positions are unique within a live session and are normalized in one
  transaction after reorder.
- Operation sequences increase monotonically within a session.
- Undo and redo commit new current state and move the operation cursor
  atomically.
- Soft deletion remains recoverable until explicit pruning.
- Only the holder of the session lease may mutate that session.
- All timestamps are stored as UTC integers and rendered in local time.

### Migrations and recovery

Migrations are forward-only and included in the binary. Before a migration, the
application acquires the exclusive schema lock, creates a SQLite backup through
the backup API, runs the migration transactionally, and performs `quick_check`.
Failure preserves the previous database and produces a recovery path.

The application refuses to open a database schema newer than it understands.
It does not attempt a best-effort downgrade. Export and explicit recovery tools
remain available without modifying the source database.

## Multiple running versions during an update

### What a Homebrew update changes

The package manager replaces the installed artifact and updates the command
link used by future launches. It does not transform a process that is already
running into the new version. A running process continues with the executable
image it loaded at startup. A newly started pane resolves the new installed
binary.

The release binary must therefore be self-contained after startup. It may read
configuration and user data from stable platform directories, but it must not
load versioned runtime assets from its Homebrew installation directory after
launch. Removing an old package directory then cannot break an existing pane.

### Mixed-version compatibility rule

Every process holds a shared schema lock for its lifetime and records:

- Application version.
- Supported storage protocol and schema range.
- Session ID and launch directory.
- Process ID plus a platform-specific process start token.
- Start time and most recent heartbeat for diagnostics.

Schema-neutral releases may run together. The release process must prove that
both binaries interpret the existing schema and operation payloads compatibly.

If a new binary requires any migration, the initial implementation uses a
conservative rule: migration is refused while another process holds the shared
schema lock. Existing processes continue normally. The new process exits cleanly
with a message listing the active sessions and versions that must be restarted.
It never kills them and never migrates the database behind them.

After the older processes close, the next new process obtains the exclusive
schema lock, backs up and migrates the database, then reopens it under a shared
lock. All subsequent launches use the new schema.

This is intentionally stricter than trying to infer whether an arbitrary SQL
change is safe for older writers. Later releases may allow explicitly declared
additive migrations through an expand-and-contract protocol, but destructive
or semantic changes always require exclusive migration.

### Update experience

The application may check the version of the installed command locally and
show `A newer version is installed. Restart this pane when convenient.` It does
not download an update, replace its executable, restart itself, or interrupt
other sessions.

Package managers own installation and update. The application may offer the
correct command as copyable guidance after detecting its installation source.
It must not assume Homebrew when installed through a release archive or Cargo.

The initial Homebrew tap should expose one canonical package identity. A
formula is the conventional choice for this open-source CLI and matches the
existing product installation goal. A binary-only cask is also technically
valid and follows the approach used by Codex. We should not publish both under
the same token because migration between package identities produces confusing
upgrade behavior.

Whichever packaging form is selected before release, the runtime and schema
lock protocol is identical. For a cask, the explicit update command is
`brew upgrade --cask <name>`. For a formula, it is
`brew upgrade --formula <name>`.

### Failure cases

- If Homebrew updates while four panes are open, all four keep running the old
  binary. The next launch uses the new binary.
- If the update has no storage change, the new launch may coexist with them.
- If it needs a migration, the new launch explains the conflict and stops. The
  four existing panes remain usable and continue saving.
- If an existing pane crashes, its operating-system locks are released. Stale
  descriptive metadata is removed when the next process verifies the lock.
- If migration fails after all panes close, the backup remains available and no
  old binary is automatically relaunched against a partially migrated store.

## Terminal rendering and input

### Rendering

Rendering is immediate-mode and derived from current state. Widgets hold no
canonical product state. The board uses one vertical flow at every width.

Natural thought height is calculated from wrapped visual lines. A viewport-aware
cap is applied only to long thoughts. The focused thought receives enough space
to keep its cursor or active content visible, subject to a minimal navigable
context around it.

### Resize

A resize invalidates the layout snapshot, not the editor model. The next render
recomputes wrapping, natural heights, caps, scroll bounds, and hit targets from
the newest terminal dimensions. Redundant intermediate resize events may be
discarded, but the final dimensions may not be skipped.

The cursor is stored as a logical grapheme position. Reflow changes only its
visual row and column. Selection anchors follow the same rule.

### Input translation

Crossterm events first pass through a keymap and mode translator. The reducer
receives intentions such as `CreateThought`, `MoveCursor`, `CutThought`, or
`ChooseDirection`, not raw keys. Mouse coordinates resolve through the latest
layout snapshot into the same intentions.

Raw modifiers are normalized into semantic modifiers. `Primary` maps to
Command on macOS and Control on Windows and Linux. Enhanced keyboard protocols
are enabled when supported so Super and Meta events can be distinguished, but
every action retains a terminal-safe fallback and a configurable binding.

`Primary+A` selects the entire current thought only in edit mode. `Primary+U`
deletes one newline-delimited logical line as a single undoable edit. Logical
line commands operate on the text model and are independent of visual wrapping.

Bracketed paste is one payload and one undoable edit. When no thought is
selected, paste creates and focuses a new thought. The application never tries
to split a paste heuristically.

## Herdr integration

Herdr is an adapter, not a runtime dependency of the core scratchpad.

The adapter:

- Detects Herdr and negotiates its client and server protocol.
- Resolves neighboring panes in all four directions.
- Independently verifies pane identity, workspace, tab, geometry, agent type,
  session identity, and interactive state.
- Invokes the semantic prompt command directly without a shell.
- Passes text as a distinct argument or standard input supported by the
  integration. It never interpolates prompt text into shell syntax.
- Applies bounded timeouts and maps JSON responses into typed results.
- Never falls back to raw key injection.

The receiving harness decides whether a prompt sent to a working agent is
queued, treated as steering, or rejected. The gateway reports that state and
the resulting receipt without inventing its own queue semantics.

Submit preserves the thought. Submit-and-remove commits deletion only after an
accepted submission receipt, and that deletion remains undoable.

## CLI and agent-facing contract

The interactive TUI and scriptable CLI call the same `SessionService`. This
avoids a second set of business rules. The repository ships a dedicated
`skills/proqi/SKILL.md` package as a thin description of this stable command
surface. Harnesses may expose it as `/proqi`, `$proqi`, or natural-language
skill invocation.

Agent-friendly commands follow these rules:

- Accept opaque IDs and text through standard input for large or arbitrary
  content.
- Provide `--json` with a versioned schema and stable machine-readable error
  codes.
- Keep human output concise and send diagnostics to standard error.
- Support discovery commands before mutation.
- Make mutation idempotent when a caller supplies an operation ID.
- Return nonzero on busy, ambiguous, unsupported, or failed operations.
- Never require parsing the TUI or terminal escape sequences.

The version 1 machine envelope is `{ schema_version, ok, data }` on success and
`{ schema_version, ok, error: { code, message, details } }` on failure. JSON is
written to standard output for both outcomes, while human diagnostics use
standard error. Thought bodies enter through standard input. A caller-supplied
`op_` identity is resolved against its typed durable request before mutation,
so matching retries return the original receipt and mismatched reuse fails.

Read-only commands may inspect the shared database through the storage facade.
A mutating CLI command first resolves the session owner. For an inactive
session it acquires the ordinary session lease. For an active session it sends
a typed request through the owner's user-only local control endpoint. The owner
turns that request into an ordinary action, then returns the durable operation
receipt. It never writes around the owner.

The local transport is a Unix-domain socket on macOS and Linux and a named pipe
on Windows. Endpoint metadata lives beside runtime lock metadata. Peer-user
validation, bounded messages, protocol negotiation, idempotency keys, and
timeouts are mandatory. If forwarding is unsupported or the owner cannot be
verified, the CLI returns `session_busy`.

The Proqi skill contains instructions and examples, not privileged executable
logic. It begins with capability discovery, passes arbitrary thought content by
standard input, requests JSON, and surfaces structured errors without parsing
terminal output. Installation of the skill does not trigger background access
or automatic scratchpad reads.

## Privacy and security

- The application has no network requirement and no telemetry by default.
- Diagnostic logs exclude thought and clipboard content by default.
- Paths and agent metadata are logged only at an explicit diagnostic level and
  are redactable in support bundles.
- Database, backup, lock metadata, and config files use user-only permissions
  where the platform supports them.
- Prompt content is never passed through a shell.
- SQLite queries use bound parameters.
- Import, export, and recovery reject unsafe path traversal.
- Dependency advisories, licenses, and release provenance are checked in CI.

There is no security boundary between the application and other processes
running as the same operating-system user. The design prevents accidental
cross-session writes and unsafe command construction, not a malicious local
administrator.

## Test architecture

### Fast deterministic tests

- Reducer tests cover every action, state transition, and effect request.
- Model tests cover operation ordering, undo, redo, reorder, and deletion.
- Layout tests cover widths, heights, Unicode, long thoughts, and resize
  invariants.
- Editor contract tests run unchanged against every editor backend.
- Facade contract tests run against in-memory fakes and real adapters.

Property tests generate edit sequences, resize sequences, and undo-redo cycles.
The invariant is that the model remains valid and a full undo returns to the
known initial state.

### Integration tests

- SQLite tests use temporary on-disk databases, real WAL mode, multiple
  connections, busy writers, interrupted commits, migrations, backups, and
  newer-schema refusal.
- Runtime tests launch multiple processes to verify session locks, shared
  schema locks, crash release, stale metadata cleanup, and mixed-version
  migration refusal.
- Herdr adapter tests use a fake executable with recorded JSON fixtures,
  protocol mismatches, delays, malformed output, and ambiguous neighbors.
- Clipboard tests use fake adapters in CI and gated platform smoke tests on
  supported desktops.

### End-to-end tests

A pseudo-terminal harness launches the real binary and drives keys, mouse
events, bracketed paste, signals, and resize sequences. It asserts semantic
screen snapshots after normalizing escape sequences and nondeterministic data.

Platform smoke tests verify terminal restoration, native clipboard behavior,
package installation, launch, session resumption, and uninstall boundaries.
Manual release checks cover real terminal emulators and multiplexers where
automation cannot faithfully reproduce host behavior.

## Engineering operations

Proqi adopts one operational contract for contributors, coding agents, CI, and
release automation. A check that matters must be executable locally through the
same command that CI runs. Workflow YAML does not become a second source of
truth for build or test behavior.

### Canonical command surface

The workspace contains a small Rust `xtask` crate. It is the cross-platform
equivalent of a project Makefile and exposes these stable entry points:

```text
cargo xtask setup
cargo xtask install-hooks
cargo xtask format
cargo xtask source-limits
cargo xtask architecture
cargo xtask check
cargo xtask test
cargo xtask test-pty
cargo xtask coverage
cargo xtask audit
cargo xtask package
```

- `setup` verifies the pinned toolchain, Rust components, and developer tools.
  It reports missing prerequisites and does not silently modify global state.
- `install-hooks` explicitly opts the current clone into the checked-in Git
  hooks. Builds and setup never change Git configuration automatically.
- `format` applies `rustfmt` and any repository-owned text formatting.
- `source-limits` rejects every first-party Rust or common frontend source file
  above 500 physical lines, including JavaScript, TypeScript, stylesheet, HTML,
  Vue, Svelte, and Astro sources.
- `architecture` verifies the inward dependency graph, canonical domain API,
  and ownership of SQLite, terminal, process, environment, and filesystem
  implementation dependencies. Its detector tests include accepted and
  rejected examples, and the scan fails if expected source layers are absent.
- `check` runs the normal pre-push gate: formatting in check mode, Clippy for
  all targets and features, source limits, documentation warnings, and the
  deterministic test suite through `cargo-nextest`.
- `test` runs the deterministic unit, contract, and integration suites.
- `test-pty` builds the real binary and runs pseudo-terminal scenarios.
- `coverage` uses `cargo-llvm-cov` and produces machine-readable and human
  reports from the same tests used in CI.
- `audit` runs dependency advisory, license, source, and duplicate-dependency
  policy through `cargo-audit` and `cargo-deny`.
- `package` builds release-mode artifacts and performs installation smoke tests
  without publishing them.

The commands remain thin orchestrators around standard Cargo tools. They print
the commands they run, propagate exit codes, avoid network access unless the
operation inherently requires it, and work on macOS, Linux, and Windows. A
failing subcommand fails the overall command immediately.

Rust uses Clippy's `cognitive_complexity` lint with a threshold of 25 as a
secondary heuristic, not as a claim to measure true cognitive complexity. The
same gate limits functions to 80 lines and nesting to four levels. These signals
work together with review and tests. A future frontend stack must establish a
well-maintained language-native complexity lint before its first source file is
merged. Every frontend source file is also subject to the repository-wide
500-line ceiling.

The checked-in pre-commit hook runs `cargo xtask check` after explicit local
installation through `cargo xtask install-hooks`. It is a convenience rather
than an enforcement boundary, with CI remaining authoritative.

### Continuous integration

Pull requests and pushes to the protected default branch run these jobs:

- `quality` runs formatting, Clippy with warnings denied, and documentation
  checks on the pinned stable toolchain.
- `test` runs the deterministic suite on macOS, Linux, and Windows. The matrix
  does not use fail-fast because all platform results are diagnostically useful.
- `msrv` compiles and tests with the declared minimum supported Rust version.
- `pty` runs terminal scenarios on each platform where the harness is supported.
- `coverage` publishes a report from Linux and enforces a 70 percent line
  threshold. Exclusions must be narrow and justified in configuration.
- `security` runs dependency advisory, license, source, and policy checks.
- `check` is an aggregate job that succeeds only when every required job has
  succeeded or has been explicitly marked inapplicable.

The aggregate `check` job is the stable branch-protection contract. Individual
jobs may evolve without repeatedly changing repository settings. Superseded
pull-request runs are cancelled, release runs are never cancelled, workflow
permissions use least privilege, and every third-party GitHub Action is pinned
to a full commit SHA with its human-readable version recorded in a comment.

Tests that require a real desktop clipboard, a specific terminal emulator, or a
live Herdr session are gated smoke tests. Their deterministic equivalents remain
required on every pull request. Live smoke tests run before releases and may also
run on a schedule without weakening the required gate.

### Dependencies and repository policy

Cargo.lock is committed because Proqi ships an application. Dependabot checks
Cargo dependencies and GitHub Actions weekly. Dependency pull requests pass the
same required gate as contributor pull requests. Automatic merging is limited
to explicitly allowed low-risk patch updates after all required checks pass.
Minor updates, all pre-1.0 compatibility changes, and security-sensitive crates
receive human review.

The default branch requires the aggregate `check` status and rejects force
pushes. Releases use a protected GitHub environment with narrowly scoped
credentials. CODEOWNERS, structured issue forms, a pull-request template,
`CONTRIBUTING.md`, `SECURITY.md`, and the chosen license are present before the
repository becomes public.

Repository instructions contain durable Proqi-specific rules and commands.
External agent skills may supplement those instructions but never replace them.
A third-party skill is treated as executable supply-chain input: its complete
instructions and scripts require review, its license must be clear, and any
vendored copy is pinned to a reviewed revision. Broad global Rust skill packs
are not a project prerequisite.

A project-local maintainer skill is introduced only after a working workflow
demonstrates repeated value, such as running PTY fixtures or performing a
release rehearsal. It remains separate from the public `proqi` skill that
teaches coding agents to use the installed application.

### Release pipeline

Every release starts from an immutable semantic-version tag after the aggregate
gate has passed. Release automation then:

- Rebuilds and tests the tagged source on every supported target.
- Uses `cargo-dist` or an equivalent reviewed tool to create native archives.
- Publishes the executable, shell completions, license and notices, checksums,
  an SBOM, and build-provenance attestations in one GitHub Release.
- Runs archive installation, launch, session-resume, and terminal-restoration
  smoke tests before marking the release complete.
- Updates the personal Homebrew tap from the published immutable artifacts.
- Refuses artifact replacement for an existing version.

Package publication has no hidden local step. A release rehearsal exercises the
complete workflow without publishing, and the release checklist records any
manual terminal-emulator or notarization verification that cannot be automated.

## Source organization

Start with one library crate plus thin binaries:

```text
Cargo.toml
src/
  lib.rs
  domain/          entities, values, invariants, operations
  application/     AppState, reducer, effects, SessionService
  ports/           Store, Editor, Clipboard, AgentGateway, runtime traits
  adapters/
    sqlite/
    terminal/
    clipboard/
    herdr/
    runtime/
  ui/              layout, rendering, keymaps, hit testing
  cli/             clap definitions, human and JSON presentation
  bin/proqi.rs
tests/
  contracts/
  integration/
  pty/
skills/
  proqi/SKILL.md
xtask/             canonical development, CI, and packaging commands
```

Split a module into another crate only when compilation boundaries, public API,
or independent reuse makes the cost worthwhile. Directory depth does not count
as architecture.

## Release compatibility contract

- Semantic versioning describes the public CLI, config, export format, and
  agent-facing JSON contract.
- Database schemas are internal but migrations are forward-only, backed up, and
  protected from mixed-version writers.
- Config fields are additive by default. Unknown fields produce an actionable
  warning or error rather than silent reinterpretation.
- Machine-readable JSON has an explicit schema version.
- Deprecations remain supported for at least one minor release before removal.
- Release archives contain one executable, license and notices, completions,
  checksums, and provenance attestations.
- Package-manager updates never delete user sessions, configuration, or backups.

## Decisions deliberately deferred

The architecture leaves these choices open until evidence or a product decision
resolves them:

- MIT, Apache-2.0, or dual licensing.
- Homebrew formula versus binary cask before the first public release. Only one
  becomes the canonical package identity.
- Exact signing and notarization scope for each platform.
- Additional multiplexer adapters.
- Cloud sync, shared editing, and a public plugin API, all of which remain out of
  scope for the initial architecture.

## External behavior references

- [Codex CLI documentation](https://learn.chatgpt.com/docs/codex/cli) documents
  standalone updates, Homebrew availability, and resumable CLI sessions.
- [Codex open-source repository](https://github.com/openai/codex) documents its
  standalone Rust executable and current `brew install --cask codex` channel.
- [Homebrew manual](https://docs.brew.sh/Manpage) defines formula and cask
  upgrades, including cask handling for running applications.
- [Homebrew Cask Cookbook](https://docs.brew.sh/Cask-Cookbook) defines binary
  artifacts as links into Homebrew's binary directory.
- [SQLite WAL documentation](https://www.sqlite.org/wal.html) defines the
  concurrency and durability behavior underlying the storage adapter.
