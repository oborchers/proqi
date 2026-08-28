# Architecture

Status: v0.1.0 architecture contract

Project: Proqi

Command: `proqi`
Last updated: 2026-08-27

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
- Cargo owns dependency resolution. A reviewed pinned `cargo-dist` release tool
  produces platform archives and metadata without becoming a second local
  development command surface.

The `v0.1.0` release targets are Apple silicon macOS, Intel macOS, and x86-64
Linux using GNU libc 2.35 or newer.

Rust provides a single native executable, predictable resource use, strong
cross-platform support, and a mature terminal ecosystem. It also makes it
possible to ship without a Node, Python, or JVM runtime.

### Terminal interface

- Ratatui owns terminal-independent drawing primitives and widget composition.
- Crossterm owns terminal setup, input, resize, mouse, bracketed-paste, and
  capability handling.
- Crossterm is temporarily pinned to audited upstream revision
  `c006ee6efbd7bed45f1286ec9545d401f3ecb1fe`, which terminates Unix event reads
  on terminal EOF and non-retryable I/O errors. Remove the pin only after an
  equivalent released version passes the PTY-close regression suite.
- Automatic mode queries the terminal foreground and background through a
  bounded terminal-adapter palette probe before alternate-screen setup and
  before the input lane starts. This ordering prevents the probe from consuming
  user typeahead. Failed probes retain terminal-native colors and a gutter-only
  focus treatment.
- `unicode-segmentation` and `unicode-width` define grapheme and terminal-cell
  behavior. Byte indices are never treated as visual columns.
- The editor port is backed by Ropey. A dependency spike against
  `ratatui-textarea` 0.9.2 confirmed useful Unicode wrapping and selection
  behavior, but the crate normalizes CRLF input and keeps wrapped mouse mapping
  behind private state. Those constraints conflict with exact paste preservation
  and layout-derived mouse hit testing. Ratatui therefore renders Proqi's own
  editor snapshot rather than owning the text model.

Terminal workers share one cancellation boundary and a two-second overall
shutdown deadline. Ordinary quit and termination cancel every lane before any
worker is joined. An accepted update replacement first closes control admission,
confirms delivery of its restart receipt, and lets the update coordinator consume
that receipt before requesting shared cancellation. Terminal restoration and
lease release are attempted independently of worker failures, and teardown
reports all failures after every cleanup has run. `SIGINT` and `SIGTERM` request
this bounded path. `SIGHUP` retains its operating-system default until a future
design can guarantee restoration after terminal revocation without keeping a
revoked input descriptor alive.

Application subprocesses start in dedicated Unix process groups. Deadline or
I/O failure sends `SIGTERM` to the group, waits 250 milliseconds, then sends
`SIGKILL`, reaps the direct child, closes its pipes, and joins bounded I/O
workers. No shell or unsafe Rust is required for this ownership boundary.

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
- `serde` with TOML for configuration and versioned machine output.

Theme configuration is parsed by the terminal adapter before terminal entry.
The UI receives one resolved semantic `Theme`; widgets never read files or
interpret user color strings. A theme recipe consists of a built-in base, an
optional versioned local TOML file, and final inline overrides. Local theme
files are bounded regular-file targets, may be reached through a symlink, and
never invoke network or shell behavior. The resolver validates every custom
role pair against Proqi's WCAG contrast policy before the terminal guard enters
raw mode. Terminals without reliable true-color support receive the built-in
limited palette instead of an inaccurate custom approximation.
- `arboard` for the native clipboard, with OSC 52 behind the same facade.
- `tracing` behind one typed diagnostics adapter. Callers emit only reviewed
  lifecycle, command, and submission-state fields. Direct tracing calls outside
  the adapter are rejected by the executable architecture policy.
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
redo, and mouse placement. Pointer selection carries explicit grapheme, Unicode
word, or logical-line granularity after the UI deterministically recognizes
single, double, and triple clicks through its injected clock. `TextPosition` is
logical and survives wrapping and resize. The facade prevents a library-specific
cursor model from leaking into the application.

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
    fn read(&mut self) -> Result<ClipboardContent, ClipboardError>;
    fn write(&mut self, text: &str) -> Result<ClipboardWrite, ClipboardError>;
}
```

Native clipboard and OSC 52 are adapter choices. Cut is an application
transaction: write the exact content first, then commit deletion. Clipboard
failure leaves the thought unchanged. `ClipboardContent` is either exact text
or a validated, bounded RGBA image.

### `AttachmentStore`

Raw clipboard images cross a separate attachment port. The filesystem adapter
encodes them as PNG into a session-scoped directory below Proqi's native data
root. Creation is private, atomic, collision-safe, and durable before an
absolute path is returned to the UI. A failed image read, validation, encoding,
or installation inserts nothing.

Terminal file drops still arrive as bracketed text. The terminal adapter
normalizes only payloads that resolve completely and unambiguously to existing
absolute files. It supports local file URLs, quoted paths, escaped whitespace,
POSIX shell-escaped punctuation, multiple paths, and Unicode names. Ordinary
prompt text remains exact. Dropped files remain external references and are
never read or copied automatically.

### `AgentGateway`

```rust
trait AgentGateway {
    fn capabilities(&mut self) -> Result<AgentCapabilities>;
    fn adjacent_targets(&mut self, context: PaneContext) -> Result<Vec<AgentTarget>>;
    fn submit(&mut self, request: SubmissionRequest) -> Result<SubmissionReceipt>;
}
```

Capabilities and targets expose typed support for immediate semantic submission.
The application separately models whether the accepted submission keeps the
source thought or removes it afterward. The gateway returns verified targets
and typed readiness states. It never returns a convenient but unverified pane.
Delivery success means the harness accepted the matching request. It does not
mean the agent finished processing the prompt. Unsupported contracts fail
before any prompt process is executed.

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

`InvocationCatalog` is a blocking port implemented by the bounded filesystem
adapter and owned by the existing external worker lane. Its durable conceptual
model separates Skill, Command, and Agent from harness provenance, project /
global / plugin scope, and optional evidence-backed invocation forms. This
prevents a Claude-specific rewrite when another documented harness adds an
equivalent layer, and prevents catalog-only definitions from being fabricated
into insertable tokens.

Discovery reads only small metadata prefixes and never stores instruction
bodies. The adapter caps roots, ancestor depth, recursion, entries, file sizes,
metadata strings, plugin registries, and manifest component paths. It follows
only explicitly encountered symlink definitions, canonicalizes physical paths
for deduplication, and never crawls the home directory. Compatibility roots are
checked in; extra roots enter through validated configuration with explicit
kind, harness, and scope. Project and global vectors remain separate.

The UI owner assigns a generation and cwd to each refresh. Results update state
only when both still match, so stale external work cannot leak an older project
catalog. Completion derives a byte range from the exact editor snapshot, moves
the existing editor selection to that range, and performs one semantic paste.
The resulting `TextChangeSet` continues through annotation rebasing and editor
undo without a parallel text-mutation contract.

An explicitly created empty thought is an ordinary durable domain entity. Its
creation is committed through the same board operation as populated thoughts,
so it participates in session ordering, resume, undo, redo, and crash recovery.
The insertion row and focused empty thoughts retain board-mode command
semantics. Content entry starts only through explicit create, edit, paste, or
pointer intentions. This prevents an empty entity from intercepting delete,
navigation, help, or configurable plain-key commands.

## Domain model and database

The database stores current state and operation history. It is not a pure
event-sourced system.

### Principal records

- `sessions`: identity, optional name, original and last-opened directories,
  timestamps, last durable operation sequence, and deletion state.
- `thoughts`: session, exact current content, validated presentation annotations,
  integer position, timestamps, durable automatic, expanded, or collapsed
  presentation preference, and deletion state.
- `thought_revisions`: coalesced text revisions with enough data to restore the
  previous and next content, annotations, and cursor state.
- `operations`: ordered structural operations and their inverse payloads for
  persistent undo and redo.
- `integration_context`: optional last-known terminal and verified agent
  metadata. Pane IDs are hints, never durable identity.
- `submission_attempts`: one content-redacted semantic delivery and its
  aggregate payload digest, target fingerprint, disposition, and state.
- `submission_attempt_items`: ordered source thought identities and per-source
  digests for one delivery. A partial unique index permits at most one active
  submission per thought.
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
- A multi-thought mutation is stored as one ordered batch with one inverse, so
  delete, duplicate, collapse, cut, and submit-and-remove remain one undo step.
- Soft deletion remains recoverable until explicit pruning.
- Only the holder of the session lease may mutate that session.
- All timestamps are stored as UTC integers and rendered in local time.
- Presentation annotations are sorted, non-overlapping UTF-8 byte ranges within
  canonical thought content. They never replace or truncate that content.

### Migrations and recovery

Migrations are forward-only and included in the binary. Before a migration, the
application acquires the exclusive schema lock, creates a SQLite backup through
the backup API, runs the migration transactionally, and performs `quick_check`.
Failure preserves the previous database and produces a recovery path.

Schema-lock acquisition waits for at most five seconds with bounded retries.
This absorbs ordinary concurrent first-launch and brief migration contention,
then returns the stable `schema_busy` error instead of waiting indefinitely.

The application refuses to open a database schema newer than it understands.
It does not attempt a best-effort downgrade. Export and explicit recovery tools
remain available without modifying the source database.

## Multiple running versions during an update

### Installation-wide update boundary

Update awareness is a typed application capability. HTTP, GitHub response
parsing, filesystem state, process execution, install detection, terminal
cleanup, and Unix process replacement remain adapters. Domain values identify
stable versions, installation identities, update choices, participants, and
operation state without containing URLs, shell strings, environment snapshots,
or terminal content.

Only interactive release builds schedule a startup check. Debug and source
builds, tests, JSON commands, skill invocations, and other noninteractive paths
do not. `proqi update check --json` and the command palette's explicit check are
the deliberate exceptions. Every eligible startup schedules a nonblocking
background refresh. Concurrent processes that observed the same private cache
generation coalesce into one request, while a later startup advances the
generation and checks again.

The HTTPS adapter uses bounded connection, response, redirect, body, and retry
limits. It requests the latest stable GitHub Release without authentication,
ignores drafts and prereleases, and sends no user content or Proqi state. The
only optional request metadata is a bounded application name and version
User-Agent required by GitHub.

Standalone checks use
`https://api.github.com/repos/oborchers/proqi/releases/latest`. Homebrew checks
read the public `oborchers/homebrew-tap` formula so they never advertise an
uninstallable release. Startup access is restricted to interactive release
builds with `check_for_updates = true`. Explicit CLI and command-palette access
is deliberate caller authority.

### Shared cache and election

Update state is separate from SQLite thought data and private to the current
user. It stores only:

- Latest stable version, refresh generation, and last successful check time.
- Exact dismissed and skipped versions.
- Last observed installed version.
- Whether an older running process may still need restart.
- Optional bounded safe HTTP cache metadata.

The cache is atomic, bounded, corruption-tolerant, and protected by an explicit
installation-wide lock. Corruption is treated as a miss. One generation
comparison and lock transaction elects a network refresher, prompt owner, and
installer coordinator, so 10 to 15 concurrent startups still produce at most
one request, one actionable prompt, and one installer. `Not now` lasts until
the next successful eligible startup refresh. `Skip this version` and
configuration opt-out update shared state atomically and are observed by every
process.

Install detection is deterministic and testable. It distinguishes
`HomebrewFormula`, `StandaloneArchive`, and `SourceOrUnknown` using the canonical
executable path plus package-owned metadata or another strong marker. A
forgeable environment variable is never sufficient by itself.

Cargo and Debian installations deliberately remain `SourceOrUnknown`. Neither
channel has an automatic installer, and Proqi never invokes `cargo`, `apt`,
`dpkg`, or `sudo` to replace itself. The Debian package therefore does not copy
the standalone archive marker into `/usr`; doing so would misidentify its owner.

### Coordination protocol

The existing current-user runtime registry and owner-control transport gain a
small ephemeral update protocol. A coordination message includes a typed
operation ID, target version, installation identity, participant identity,
deadline, and one of prepare, ready, blocked, installation-result, or restart
requests. Messages are bounded and contain no prompt text, arbitrary command,
terminal content, secret, or raw environment data.

The coordinator verifies current-user ownership, process start identity, live
session lease, endpoint, compatibility domain, and installation identity. It
rejects stale records, PID reuse, forged endpoints, and other users. It snapshots
live participants for preflight and rescans after installation so a process
created during the update can converge without a durable distributed phase log.

Every preflight participant flushes durable thoughts, drafts, and the ordinary
resume identity and UI state. A save failure, negative acknowledgement, live
timeout, or lost coordinator aborts before installation. Ready participants
return to their prior session after a bounded timeout. The shared cache records
only the minimal state a later process needs to compare installed and running
versions.

### Homebrew installation and Unix process replacement

Homebrew is the sole owner of installed-file replacement. On macOS and Linux,
one coordinator directly executes exactly:

```text
program: brew
arguments: upgrade, --formula, oborchers/tap/proqi
```

No shell is involved and Proqi never overwrites a Homebrew-managed executable.
If installation fails or the result is ambiguous, no participant calls `exec`.
Every old process returns to normal use after a bounded wait.

After success, the coordinator rescans active instances and publishes the
installed version. It addresses peer participants first and its own process
last, so local shutdown cannot interrupt remaining restart requests. A
participant reserves the matching restart, closes new control admission, and
commits to shutdown only after the accepted receipt frame has been written to
the verified local socket. A failed delivery leaves that participant running
and records restart convergence as incomplete.

Each participant then independently restores terminal modes, stops worker
threads, closes control transport, releases session and schema leases, applies
an explicit descriptor policy, resolves and verifies the active Homebrew Proqi
path, and calls Unix `exec` with its ordinary resume arguments. Cleanup is
explicit because successful `CommandExt::exec` does not run Rust destructors.
Standard input, output, error, and the inherited PTY remain attached, so no
shell, terminal multiplexer, Herdr, or parent agent must recreate the pane.

The replacement invocation preserves the ordinary resume identity and any
explicit state-root argument. This keeps package tests and portable invocations
on the same data paths instead of silently falling back to platform defaults.

A failed `exec` does not undo successful peers. Where safe, the old process
re-enters its session; otherwise its durable state remains normally resumable.
Runtime metadata marks it as an old-version participant and the UI offers a
direct retry. The system never reports complete restart while such an instance
remains.

### Schema compatibility during convergence

Every running process holds the existing shared schema lease and records its
application and storage protocol versions. Schema-neutral versions may coexist
only when release tests prove compatible payload interpretation. A process that
requires migration must obtain the exclusive schema lease, create a verified
backup, migrate transactionally, and pass integrity checking.

An old process holding a shared lease prevents an incompatible migration. A new
process waits within the update convergence window or reports a bounded
restart-pending state and retries after the old process leaves. It never migrates
behind an older writer. This conservative barrier remains mandatory even though
the public CLI has no compatibility guarantee before `1.0`.

### Standalone, Debian, Cargo, and unknown installations

Standalone archives share version checking, prompt election, global dismissal,
checkpointing, and ordinary resumable sessions. `v0.1.0` does not replace an
archive executable or guarantee same-pane restart. It provides a verified stable
release URL and external replacement instructions, then resumes on the next
normal start. `SourceOrUnknown` installations receive accurate non-destructive
guidance or no action.

Automatic standalone replacement remains behind a future updater port. It must
not be approximated by writing over the running executable, invoking `curl`, or
assuming a package manager.

Debian and Cargo installations use their external package managers only through
documented user commands. The Debian artifact is a directly downloaded local
package, not an APT repository. Removing it deletes package-owned files only and
never user state. Cargo publication distributes the `proqi` binary and does not
make the internal library a supported API.

## Terminal rendering and input

### Rendering

Rendering is immediate-mode and derived from current state. Widgets hold no
canonical product state. The board uses one vertical flow at every width.

Automatic mode preserves the detected terminal foreground and background. Its
neutral selected surface is derived by blending the background eight percent
toward white on dark terminals or black on light terminals. Explicit dark and
light themes use fixed neutral selected surfaces. Primary and accent text pairs
are checked against WCAG AA contrast thresholds. Failed automatic detection
and limited-color terminals retain the non-color gutter cue without inventing
an unsafe contrast pair.

The board reserves independent responsive regions for product and session
identity, content, integration or durability context, contextual actions, and
verified adjacent-agent targets. Transient status shares the context row:
information and success compose beside the summary, while warnings and errors
replace it temporarily. Renderers never paint
independently aligned strings into the same cells. Compact panes shorten labels
and remove secondary context before they reduce the usable content area below
one row.

The ordinary board has no permanent top header. Its responsive footer is the
single session-summary surface and contains name, thought count, mode, and
durability when width permits. Agent controls are content-sized and only exist
for independently verified targets.

Natural thought height is calculated from wrapped visual lines. A viewport-aware
cap is applied only to long thoughts. The focused thought receives enough space
to keep its cursor or active content visible, subject to a minimal navigable
context around it. The maximum board scroll position includes the insertion row
as a terminal virtual item, so the final page always exposes `+ New thought`
above the footer without permitting blank overscroll.

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
Command on macOS and Control on Linux. Enhanced keyboard protocols
are enabled when supported so Super and Meta events can be distinguished, but
every action retains a terminal-safe fallback and a configurable binding.

`Primary+A` selects the entire current thought only in edit mode. `Primary+U`
deletes one newline-delimited logical line as a single undoable edit. Logical
line commands operate on the text model and are independent of visual wrapping.

Bracketed paste is one payload and one undoable edit. When no thought is
selected, paste creates and focuses a new thought. The application never tries
to split a paste heuristically.

Board-mode printable keys always pass through the configured command map, even
when the insertion row or a durable blank has focus. The second blocked
downward movement at the end of a non-empty final edited thought creates a
durable blank and enters its editor. Repeated movement while that blank remains
empty cannot create additional thoughts. On the insertion row, two consecutive
semantic downward navigation commands perform the same durable create-and-edit
transition; unrelated or reorder input clears the confirmation. Other edit
boundaries use the same navigation state machine.

The normalized paste payload carries exact text plus optional typed provenance.
Attachment annotations retain only presentation-safe metadata and byte ranges;
the absolute path remains the canonical text. Large-paste annotations retain
derived line and grapheme counts. A UI-owned projection substitutes folded
labels for both board rendering and editor snapshots, while the editor model,
clipboard, recovery, CLI, search, and integration boundaries continue to
consume canonical content. The projection owns lossless canonical-to-visible
cursor and selection mapping. Collapsed ranges are atomic for pointer, cursor,
selection, and deletion commands. Edits rebase unaffected ranges and dissolve
overlapping ranges. Revisions persist both sides of the annotation change so
undo and redo remain restart-safe.

URL recognition is a render-only pass over canonical content. Only explicit
HTTP and HTTPS ranges receive accent and underline styling. URL recognition does
not create durable annotations, rewrite content, or participate in editor
position conversion.

## Herdr integration

Herdr is an adapter, not a runtime dependency of the core scratchpad.

The adapter:

- Detects Herdr and negotiates its client and server protocol.
- Resolves neighboring panes in all four directions.
- Independently verifies pane identity, workspace, tab, geometry, agent type,
  optional session identity, and interactive state.
- Invokes the semantic prompt command directly without a shell.
- Passes text as a distinct argument or standard input supported by the
  integration. It never interpolates prompt text into shell syntax.
- Applies bounded timeouts and maps JSON responses into typed results.
- Never falls back to raw key injection.

The receiving harness decides whether a prompt sent to a working agent is
queued, treated as steering, or rejected. The gateway reports that state and
the resulting receipt without inventing its own queue semantics.

Both visible actions invoke the same immediate semantic prompt command.
`SubmissionDisposition::Keep` preserves the thought.
`SubmissionDisposition::RemoveAfterSuccess` commits deletion only after an
accepted receipt whose submission identifier and target match the pending
request. Matching uses stable target identity fields and deliberately ignores
volatile readiness, display names, and geometry observed after delivery. The
explicit provisional transitions permit sessionless Codex, Kilo, or OpenCode
requests to match a receipt that preserves pane and agent identity, whether the
receipt already contains the new session or precedes the session hook.
Established sessions still require exact identity.
The matching `agent_prompted` receipt establishes acceptance. Any
post-submit agent state is advisory, including `blocked`, `unknown`, or no
reported state. The accepted outcome is journaled durably before an unchanged
thought may be removed. That deletion remains undoable. Every failure preserves
the thought.

Submission attempts use a content-redacted SQLite journal. Proqi first reserves
every source thought in `prepared`, compare-and-sets the attempt to `sending`,
invokes Herdr once with no open database transaction, then compare-and-sets a
terminal result. Only one active attempt may reference a thought. Recovery
changes `prepared` to `cancelled` and `sending` to `outcome_unknown`; it never
automatically retries an ambiguous delivery. The journal stores ordered source
identities, their SHA-256 digests, one aggregate payload digest, and a target
identity fingerprint, never prompt content or raw pane and agent session
identifiers.

`v0.1.0` implements this boundary against Herdr's structured schema
and protocol discovery commands. Capability discovery verifies both the
`agent.prompt` request and `agent_prompted` receipt shapes. Explicit
`interactive_ready=false` or `launch_pending=true` metadata makes a target
ineligible, while absent optional readiness metadata remains compatible. It
fails closed when the installed client and server no longer match the supported
contract. Initial discovery is silent so
ordinary terminals retain an uncluttered board. An explicit refresh, or a
submission attempt with no verified target, reports why direct submission is
unavailable. Every submission revalidates the complete target immediately
before invoking Herdr's semantic prompt operation. Ready sessionless Codex,
Kilo, and OpenCode targets are eligible provisionally; other sessionless
agents are hidden. A valid session from the matching receipt replaces the
provisional target immediately. When the receipt precedes the session hook,
Proqi accepts the
matching receipt and immediately rediscovers adjacent targets without retrying
the prompt.

Herdr protocol 19 acknowledges accepted text entry but does not guarantee a
distinct prompt boundary when another sender submits concurrently. This is a
known provider-contract limitation. Proqi retains target verification, receipt
matching, durable journaling, and remove-only-after-acceptance semantics, but
cannot prevent the receiving harness from merging overlapping inputs. Protocol
19 also cannot atomically reject replacement of one supported sessionless
harness by another instance of the same kind in the same pane between
revalidation and delivery because it exposes neither a pre-session instance
identity nor an expected-instance precondition.

Herdr also implements a separate display-only `PanePresentation` port. In a
managed pane, the terminal runtime publishes `title=proqi` and
`display-agent=proqi` through `pane report-metadata` without the agent identity
field. A monotonically increasing sequence refreshes a 15-second TTL every 10
seconds. Clean shutdown clears both fields; crash recovery relies on expiry.
Focus-gained events refresh target discovery immediately, while resize bursts
trigger one debounced refresh after geometry settles. Metadata failure never
weakens the standalone board or changes submission verification.

## CLI and agent-facing contract

The interactive TUI and scriptable CLI call the same `SessionService`. This
avoids a second set of business rules. The repository ships a dedicated
`skills/proqi/SKILL.md` package as a thin description of the installed version's
current JSON command surface. Harnesses may expose it as `/proqi`, `$proqi`, or
natural-language skill invocation.

Agent-friendly commands follow these rules:

- Accept opaque IDs and text through standard input for large or arbitrary
  content.
- Provide `--json` with a versioned schema and documented current-version error
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

Read-only commands synchronize with a compatible active owner before inspecting
the shared database through the storage facade. When a legacy owner predates
the synchronization request, reads remain available from its last durable
SQLite state instead of being misreported as a busy mutation. A mutating CLI command first
resolves the session owner. For an inactive session it acquires the ordinary
session lease. For an active session it sends a typed request through the
owner's user-only local control endpoint. The owner turns that request into an
ordinary action, then returns the durable operation or metadata receipt. It
never writes around the owner.

The local transport is a Unix-domain socket on macOS and Linux. There is no
insecure fallback. Endpoint metadata lives beside runtime lock metadata. Peer-user validation,
bounded messages, protocol negotiation, idempotency keys, and timeouts are
mandatory. If forwarding is unsupported or the owner cannot be verified, the
CLI returns `session_busy`.

Control protocol version 4 supports durable presentation annotations, session
rename, owner synchronization, exact editor replacement, and durable collapse
state. Exact replacement carries a typed `rev_` idempotency identity plus either
the caller's expected SHA-256 content digest or an explicit force intention and
enters the ordinary editor revision history. The owner rejects every mutation of a source thought while its
submission is in flight. Cross-session delivery inspects the source, commits an
idempotent destination creation through the verified owner or an acquired
inactive-session lease, and only then requests an ordinary source deletion. No
direct database write bypasses an active destination owner.

The Proqi skill contains instructions and examples, not privileged executable
logic. It begins with capability discovery, passes arbitrary thought content by
standard input, requests JSON, and surfaces structured errors without parsing
terminal output. Installation of the skill does not trigger background access
or automatic scratchpad reads.

## Privacy and security

- Ordinary product use has no network requirement. Interactive release builds
  may perform the bounded, disableable GitHub stable-version check described
  above. Proqi has no telemetry.
- Update requests contain no thought, clipboard, path, session, identifier,
  runtime, terminal, or usage data.
- Diagnostic logs exclude thought content, clipboard content, session names,
  workspace paths, pane identifiers, and raw external responses.
- Each instance owns a locked JSONL stream with five 1 MiB segments. Startup
  prunes inactive streams toward a 20 MiB installation-wide ceiling without
  deleting active logs.
- Explicit diagnostic collection writes one versioned, user-private local JSON
  bundle, refuses overwrite, and performs no upload.
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
Representative in-memory terminal states also use checked-in Insta snapshots
that preserve text, spacing, foreground, background, and modifiers. Visible UI
changes require explicit snapshot review, and the canonical gate rejects every
pending `.snap.new` file.

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
- `check` runs the normal pre-push gate: formatting in check mode, Git
  whitespace validation for unstaged, staged, and committed HEAD content,
  Clippy for all targets and features, source limits, reviewed-snapshot policy,
  documentation warnings, and the deterministic test suite through
  `cargo-nextest`.
- `test` runs the deterministic unit, contract, and integration suites.
- `test-pty` builds the real binary and runs pseudo-terminal scenarios.
- `coverage` uses `cargo-llvm-cov` and produces machine-readable and human
  reports from the same tests used in CI.
- `audit` runs dependency advisory, license, source, and duplicate-dependency
  policy through `cargo-audit` and `cargo-deny`.
- `package` builds the host release executable, stages the exact standalone
  archive layout, generates Bash, Zsh, and Fish completions from the installed
  executable, and runs the copied binary from isolated config, data, cache,
  runtime, and working directories. Its installed-product contract covers exact
  version and JSON behavior, Unicode and whitespace fidelity, process-to-process
  reopen, active-owner forwarding, migration backup, newer-schema refusal,
  terminal restoration, fake update installation, same-PTY Unix replacement,
  and failure recovery. The deterministic 15-participant update test remains in
  the normal suite. Package output stays below ignored `target/package` and is
  never published by this command. Hosted release jobs may supply one
  pre-generated union notice file through `--notices`; local packaging generates
  the same notices itself.
- `crate-package` runs credential-free Cargo package and publication dry runs,
  verifies an exact source-only member allowlist and normalized manifest,
  installs from the extracted package into isolated Cargo state, and records
  the `.crate` checksum and evidence without publishing.
- `debian-package` consumes the verified x86-64 GNU/Linux archive and produces
  `proqi_amd64.deb` from the identical executable. `verify-debian` proves its
  metadata, derived dependencies, contents, permissions, lack of maintainer
  scripts, and disposable install, remove, state-preservation, and reinstall
  behavior on pinned Ubuntu 22.04, Ubuntu 24.04, and Debian bookworm images.

The commands remain thin orchestrators around standard Cargo tools. They print
the commands they run, propagate exit codes, avoid network access unless the
operation inherently requires it, and work on macOS and Linux. A
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
- `test` runs the deterministic suite on macOS and Linux. The matrix
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

New coding-agent harnesses use
[`HARNESS_QUALIFICATION_CHECKLIST.md`](HARNESS_QUALIFICATION_CHECKLIST.md) for
deterministic contract evidence, live Herdr user stories, and cleanup.

### Dependencies and repository policy

Cargo.lock is committed because Proqi ships an application. Dependabot checks
Cargo dependencies, GitHub Actions, and the pinned Rust toolchain weekly, applies
a routine update cooldown, and limits routine dependency work to one grouped
pull request. Security updates remain exempt from cooldown. Dependency pull
requests pass the same required gate as contributor pull requests. Automatic
merging is limited to explicitly allowed low-risk patch updates after all
required checks pass.
Minor updates, all pre-1.0 compatibility changes, and security-sensitive crates
receive human review.

The default branch requires the aggregate `check` status and rejects force
pushes while allowing direct owner pushes. Releases use a protected GitHub
environment with narrowly scoped credentials. CODEOWNERS, structured issue
forms, a pull-request template, `CONTRIBUTING.md`, `SECURITY.md`, Contributor
Covenant 2.1, and the MIT license are present before the repository becomes
public. Issues and pull requests are the public collaboration surfaces;
Discussions and a support mailbox remain disabled. Security reports use GitHub
private vulnerability reporting and support only the latest stable release.
Inbound contributions use the repository's MIT terms with no CLA or DCO.

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

Every release candidate starts from a protected stable tag whose exact commit
has passed the aggregate `main` gate. The tag-triggered release workflow calls
the candidate workflow as a reusable workflow, so the expensive matrix and
promotion share one run and one immutable artifact set. A manual candidate
dispatch remains available only as a non-publishing preflight or recovery tool.
Both entry points accept an exact `vX.Y.Z` tag, reject a tag that differs from
the single Cargo workspace version, and record the source ref and commit. The
matrix builds only Apple silicon macOS, Intel macOS, and x86-64 GNU Linux
artifacts on native runners. The GNU/Linux candidate is built on Ubuntu 22.04,
must not require a glibc symbol newer than `GLIBC_2.35`, and is started from its
final archive on Ubuntu 22.04, Debian bookworm, and Ubuntu 24.04. One Linux job
generates a union third-party notice file for all targets, so Intel macOS never
compiles the packaging tool.

A reviewed pinned `cargo-dist` configuration or equivalent narrow Rust tool
stages archives containing one executable, MIT license, required notices, and
shell completions. Jobs create and verify SHA-256 manifests, SPDX JSON SBOMs,
and GitHub OIDC Sigstore provenance attestations. Every third-party Action is
pinned by full commit SHA and ordinary CI remains read-only.

The candidate workflow creates a seven-day immutable artifact only after every
target, installed smoke, crate dry run, Debian package contract, checksum, SBOM,
attestation, formula, and manifest step succeeds. The Debian package reuses the
verified Linux archive executable byte for byte. The manifest separates public
release files from private crate and Debian evidence and binds the future tag,
source commit, build run, workflow, target registry, filenames, and file
digests. Promotion downloads the candidate produced by the same tag run,
verifies every internal hash and candidate attestation, adds tag-bound
attestations, and publishes the same bytes. It never rebuilds successful native
jobs. A failed promotion can be rerun while retaining their candidate artifacts.
Release creation is idempotent for absent releases,
matching drafts, and already published identical assets. Conflicting assets
fail closed. GitHub Release notes are the only changelog. The protected release
environment has no manual approval gate. Release runs are never cancelled and
existing assets for a version are immutable.

The same protected promotion job publishes the verified crate through
crates.io trusted publishing. The crate trusts only the Proqi repository,
`release.yml`, and the `release` environment. GitHub OIDC is exchanged through
the pinned official crates.io action for one short-lived token, so no
long-lived registry credential exists in CI. Promotion creates or verifies the
GitHub Release draft first, reproduces and compares the candidate `.crate`,
publishes only an absent version, verifies the public registry digest, and
installs the exact registry version into disposable Cargo state. Existing
matching registry bytes make a retry idempotent; mismatched bytes fail closed.
The GitHub Release becomes public only after this registry contract succeeds.
Promotion then downloads every public asset and requires exact byte identity
with the candidate before Homebrew is notified.

Homebrew tap updates occur only after the referenced Release assets, checksums,
and attestations are verified. The external `oborchers/homebrew-tap` repository
contains `Formula/proqi.rb` and owns an event-driven plus manually dispatchable
sync workflow. That workflow uses only its short-lived repository `GITHUB_TOKEN`,
refuses downgrades and conflicting same-version content, tests the candidate
formula before committing it, and performs exact-version no-ops. Proqi stores no
cross-repository credential. Homebrew Core remains outside scope.

Package publication has no hidden local step. A credential-free rehearsal plans
all three targets, builds and smokes the host artifact, and generates host
checksums, completions, notices, SPDX output, and formula metadata under
`target`. It reports platform work that only CI can verify. No paid platform
signing or notarization is performed.

## Source organization

Start with one library crate plus thin binaries:

```text
Cargo.toml
src/
  lib.rs
  domain/          entities, values, invariants, operations
  application/     AppState, reducer, effects, SessionService
  ports/           Store, Editor, Clipboard, AttachmentStore, AgentGateway, runtime traits
  adapters/
    sqlite/
    terminal/
    clipboard/
    attachment/
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

- Before `1.0`, Proqi does not guarantee compatibility for the human CLI,
  configuration, export format, or agent-facing JSON contract between minor
  releases. Machine JSON remains explicitly versioned and the skill discovers
  the installed version before acting.
- Database schemas are internal but migrations are forward-only, backed up, and
  protected from mixed-version writers.
- Unknown config fields produce an actionable error rather than silent
  reinterpretation.
- Machine-readable JSON has an explicit schema version.
- Breaking pre-`1.0` behavior is called out in GitHub Release notes.
- Release archives contain one executable, MIT license, notices, and
  completions. The Debian package contains the same Linux executable plus
  conventional system completions and notices. Checksums, SPDX SBOMs, and
  provenance attestations accompany every released binary artifact.
- Package-manager updates never delete user sessions, configuration, or backups.

## Decisions deliberately deferred

The architecture leaves only later product expansion open:

- Additional multiplexer and coding-agent adapters.
- A separately reviewed standalone self-replacement mechanism after `v0.1.0`.
- Cloud sync, shared editing, and a public plugin API.
- Homebrew Core submission after the project independently meets its policy.

## External behavior references

- [GitHub Releases REST API](https://docs.github.com/en/rest/releases/releases)
  defines stable release discovery.
- [Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook) defines
  formula installation and upgrade behavior.
- [Homebrew Acceptable Formulae](https://docs.brew.sh/Acceptable-Formulae)
  records the policy boundary for a possible later Core submission.
- [POSIX exec](https://pubs.opengroup.org/onlinepubs/9799919799/functions/exec.html)
  defines same-process image replacement.
- [Rust Unix CommandExt](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html)
  documents that successful `exec` does not run Rust destructors.
- [SQLite WAL documentation](https://www.sqlite.org/wal.html) defines the
  concurrency and durability behavior underlying the storage adapter.
