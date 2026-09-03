# Architecture

Status: v0.1.0 architecture contract

Project: Proqi

Command: `proqi`
Last updated: 2026-09-01

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
- Crossterm 0.29 is resolved from crates.io. The PTY-close regression suite
  continuously verifies that Unix event reads terminate on terminal EOF and
  non-retryable I/O errors.
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

The process has seven logical lanes:

1. The UI lane reduces actions, computes layout, and renders the latest state.
2. The input lane reads terminal events and sends normalized input messages.
3. The storage lane owns one SQLite connection and executes ordered persistence
   commands.
4. The external-effects lane handles clipboard and integration calls with
   explicit timeouts.
5. The update lane owns bounded installation discovery and explicit updates.
6. The macOS screenshot lane owns bounded directory reconciliation; on Linux it
   exposes only the typed unsupported result and starts no watcher.
7. The attachment-accessibility lane executes ordered, bounded readability
   checks through the injected filesystem adapter. It owns no cache, trigger,
   presentation, or submission policy.

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

Screenshot creation is deliberately commit-first rather than optimistic. The
application-owned capture constructor stores the exact source path followed by
one ASCII space while its attachment annotation and health key cover only the
path. The storage lane atomically inserts one capture receipt and its exact
append operation in detection order. The UI applies the operation only after
that transaction succeeds. The receipt's rename-stable source fingerprint prevents
duplicate delivery across repeated notifications, reconciliation, retry,
restart, and ownership handoff. Failure leaves no partial thought and retains
retryable work in the live session. An ordinary store failure initiates bounded
watcher stop and operating-system lease release; retained retry work never
monopolizes installation-wide capture authority.

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

One editor instance owns transient editing state under a typed UI owner. The
owner is either `Compose` or one durable `ThoughtId`. `Compose` contains only an
editor buffer and a typed UI-only `Prompt | Editor` presentation. `Prompt`
suppresses the editor projection while retaining the empty Compose owner;
`Editor` exposes the ordinary editor surface. Neither presentation owns a
domain entity, operation, or persistence identity. Promoting Compose replaces
the owner in place after the canonical populated create succeeds, which retains
the exact cursor, selection, annotations, viewport, and first input.

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

Layout is a pure function of board state, typed editor-owner state, terminal
capabilities, and viewport dimensions. It returns a `LayoutSnapshot` containing
rectangles, wrapped visual lines, scroll bounds, focus geometry, and mouse hit
targets. Engaged Compose uses the same editor measurement and rendering path at
the insertion row without synthesizing a durable thought. Passive Compose omits
that editor projection and instead uses the canonical insertion-row geometry for
the centered `+ Start typing` prompt and its whole-row hit target.

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

### `AttachmentAccessibility`

External attachment health crosses a terminal-independent read-only port. The
filesystem adapter opens the exact absolute path without rewriting it, proves
that it is a readable regular file, and returns typed missing, permission,
unmounted, unreadable, or I/O failures. The bounded lane adds timeout and
cancellation failures without waiting for a blocked filesystem call to return.
Those reasons are content-free diagnostics only. Application and UI consumers
reduce every completed failure to binary inaccessible health. Unknown and
checking state remain visually neutral without becoming accessibility proof.

Application state owns the transient exact-key cache, its explicit unknown,
checking, accessible, and inaccessible states, and the scheduling policy.
Keys include the thought, annotation index and range, presentation metadata,
canonical path, and digest of the canonical content revision. Insertion and
relink mutations invalidate affected work. Restoration schedules the focused
thought first, then bounded board batches. Real thought-focus transitions move
unknown queued work forward. Debounced host focus, the bounded inactivity
fallback, and the explicit `Refresh attachments` action invalidate and rescan
without polling, directory watchers, or render-time filesystem access.

Submission captures exact source thoughts and attachment keys before waiting
for durability. Once pending edits are durable, a fresh bounded preflight owns
the accessibility lane ahead of background continuation. The locked sources
must still match both their content digests and attachment keys when the check
completes. Any inaccessible, timed-out, cancelled, incomplete, or stale result
releases the source locks without preparing a journal attempt or invoking an
agent gateway.

### `ScreenshotWatcher`

The first Screenshot Inbox is a macOS-only, explicitly activated adapter. Its
injected watcher factory opens one configured directory, starts a `kqueue`
directory watch before taking the activation baseline, and treats vnode events
only as reconciliation hints. Identity uses device, inode, and birth time;
unchanged size and modification time across conservative observations establish
stability. Reconciliation opens entries relative to the watched directory with
no-follow semantics and accepts only bounded, magic-validated PNG, JPEG, or TIFF
regular files. Linux implements the port only as truthful unsupported behavior.

Reconciliation has a cheap identity phase and an expensive eligibility phase.
The activation baseline and already-delivered identities are rejected before
xattr or bounded image-header reads. Stable eligibility is cached until path,
size, modification time, or identity changes. A rename is part of the stable
observation and restarts the full monotonic debounce interval; dot-prefixed and
macOS-hidden staging entries are ignored. Directory enumeration, cancellation,
and follow-up work remain explicitly bounded, while an idle kqueue timeout with
no pending candidate does not rescan the directory.

`com.apple.metadata:kMDItemIsScreenCapture` is the strong language-independent
best-effort signal. User-configured filename patterns are fallbacks, and broad
new-image capture is an explicit opt-in. The watcher never captures a screen,
mutates system preferences, uploads an image, or copies or rewrites its source.
Only the bounded header required to validate type and dimensions is read.

A terminal-independent activity policy always bounds a listening lease by a
positive inactivity interval and positive unattended-admission count. The UI
owner observes an injected process-relative monotonic clock. Deliberate input
renews both bounds; passive terminal and watcher events do not. Candidate
allowance is reserved on ordered admission rather than durable success, so
retries cannot reopen capacity and one watcher batch cannot cross the hard cap.
Automatic pause reuses the ordinary bounded final reconciliation, durable drain,
and capture-lock release path. Resume creates a new watcher and baseline rather
than replaying the paused interval.

The application exposes one typed accounting model for every asynchronous UI
intention that may still allocate a session sequence: clipboard cut and paste,
remove-after-success submission, and remove-after-success transfer. The runner
combines that model with unresolved control lookup, update preparation,
persistence, and capture reservation state. An in-flight capture retryably
rejects sequence-producing owner-control requests, including sync; a capture
cannot reserve until every earlier asynchronous producer has completed or
failed. Update preparation waits in its existing owner queue rather than
aborting installation-wide coordination. The UI uses the same reservation as a
bounded ordered replay barrier for local keyboard, paste, click, drag, and
scroll intentions. Capacity applies backpressure at the input-lane boundary;
pointer motion remains passive and resize alone may coalesce.
Capture application itself changes only terminal-independent durable state; UI
composition alone decides whether the new thought may safely receive focus.
Failed ready candidates do not retain the installation lease, and retry,
disable, resume, takeover, and quit remain separate explicit lifecycle actions.
Stopped and releasing are distinct runtime states, so immediate re-enable cannot
mistake the same process's releasing lease for another owner. Shutdown sends a
typed remaining budget to watcher teardown, stops new admission, and drains all
already-emitted plus final-reconcile candidates within the shared deadline.

After the watcher has truthfully stopped, the application may emit one typed
content-free pause-notification effect. Notification routing is disabled by
default and selected only at runtime composition. A managed Herdr pane queues a
direct, shell-free `herdr notification show` request on the bounded cancellable
external lane; Herdr command construction remains owned by the Herdr adapter.
Outside Herdr, the terminal adapter writes bounded OSC 9 only for recognized
standalone Ghostty or iTerm2 hosts outside known tmux transport. A managed pane
with Herdr integration disabled uses neither route, and a failed Herdr request
never falls back to OSC. Output contains fixed product text plus the typed
numeric threshold. Queue, process, timeout, cancellation, rejection, and write
failure are non-fatal because persistent TUI state, not external presentation,
is the safety boundary.

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
update compatibility, and a separate current-user installation-wide screenshot
capture lease. Its locks are independent from SQLite transactions and from one
another.

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

Screenshot capture metadata is bounded and content-redacted. Exactly one live
process may own capture. A compatible contender can request takeover only
through the verified owner-control endpoint. The owner confirms scheduling,
stops watcher admission, performs final reconciliation, drains atomic capture
commits, and then releases the OS lease. Missing, incompatible, or still-live
owners are never force-unlocked.

### `TerminalSession`

This facade enters and restores terminal modes through an RAII guard. Setup and
teardown cover raw mode, alternate screen, bracketed paste, mouse capture,
cursor state, and panic restoration. Restoration is attempted on normal exit,
error return, panic, and termination signals supported by the platform.

### Small deterministic ports

`Clock`, `MonotonicClock`, `IdGenerator`, `Paths`, and `ProcessRunner` are injected wherever
wall-clock time, identifiers, platform directories, or subprocess execution
would otherwise make tests nondeterministic.

`InvocationCatalog` is a blocking port implemented by the bounded filesystem
adapter and owned by the existing external worker lane. Its durable conceptual
model separates Skill, Command, and Agent from harness provenance, project /
global / plugin scope, and optional evidence-backed invocation forms. This
prevents a Claude-specific rewrite when another documented harness adds an
equivalent layer, and prevents catalog-only definitions from being fabricated
into insertable tokens.

Markdown discovery opens a regular definition and reads at most a 64 KiB
frontmatter prefix. It stops as soon as the complete closing delimiter is
known, never reads or stores the instruction body, and fails closed when an
opened header does not close within that budget. A filename-derived Markdown
command without frontmatter stops as soon as the absent opening delimiter is
known. Skills and Markdown agents still require their valid metadata. Invalid
UTF-8 inside frontmatter is rejected, while bytes after a valid closing
delimiter are outside discovery and cannot invalidate it.

Formats that require complete parsing retain separate whole-file bounds,
including TOML agents, plugin manifests, and plugin registries. The adapter also
caps roots, ancestor depth, recursion, entries, metadata lines and strings,
manifest component paths, and plugin counts. It follows only explicitly
encountered symlink definitions, canonicalizes physical paths for
deduplication, and never crawls the home directory. Compatibility roots are
checked in; extra roots enter through validated configuration with explicit
kind, harness, and scope. Project and global vectors remain separate.

`InvocationReferenceCatalog` extends this discovery boundary with typed,
ephemeral collaborator locations. The Herdr adapter implements it from one
bounded protocol 19 snapshot and projects only agent name, harness, correlated
workspace and tab identity, pane identity, and observed state. Raw snapshot
JSON, directories, terminal titles, prompt text, and other privacy-sensitive
fields remain adapter-local. Workspace and tab labels are accepted only from
matching topology records in the same snapshot. Missing label collections use
exact IDs, while contradictory or duplicate identities fail closed.

Filesystem and live discovery share the bounded external worker lane but use
independent requests and completions. Filesystem refreshes retain their own
UI-assigned generation and cwd. Each manual or automatic picker open allocates
a separate live generation, clears the preceding live projection, and accepts
only the matching newest completion. A timeout or malformed completion carries
that same generation, so an old failure cannot erase newer references. Closing
the picker invalidates pending live work. There is no continuous refresh while
the picker remains open, and live failure never replaces usable filesystem
entries.

The UI stores only the typed live projection, bounds the subset, and renders one
`Live in Herdr` section through the picker's existing two-field row. The primary
and secondary projections deduplicate session name, topology labels, harness,
pane, and observed state, with responsive secondary fallbacks that retain
location first. Numeric-only tab labels are not presented as user-facing
worktree names. The exact workspace, tab, and pane identities remain in the
insertion text. Observed state is rendered from the picker-open snapshot but
deliberately omitted from inserted text. No reference selection reaches the
Herdr submission, focus, reservation, or mutation ports.

Filesystem results update state only when generation and cwd still match, so
stale external work cannot leak an older project catalog. Completion derives a
byte range from the exact editor snapshot, moves the existing editor selection
to that range, and performs one semantic paste.
The resulting `TextChangeSet` continues through annotation rebasing and editor
undo without a parallel text-mutation contract.

The UI composes a small data-driven shared-command table beside catalog results:
`/plan` and `/goal` are available only at byte zero when verified adjacent Codex
or Claude Code targets exist. They remain ordinary Command choices rather than
fabricated filesystem evidence. Outbound multi-thought assembly for either
harness keeps the complete leading shared starter only on the first thought and
removes a `/plan` or `/goal` token plus one separator from later thought starts.
It never rewrites stored sources, partial names, leading whitespace, or in-body
text.

Invocation forms carry their receiving harness independently from the source
ecosystem. When verified adjacent targets map to known harnesses, completion and
render-only highlighting filter forms to that target set; with no known target,
the authoring catalog remains available. Highlight ranges are recomputed from
exact bounded tokens and decorate terminal cells with the existing annotation
semantic role without changing editor text, cursor geometry, persistence, or
undo. Forms retain harness-specific precedence. A `.claude/skills` symlink into
the corresponding physical `.agents/skills` definition contributes its Claude
form to the Agent Skills-owned entry. This remains true when the Agent Skills
entry is itself a supported symlinked skill folder whose final definition is
outside the root; independent aliases to the same external target do not
establish ownership, and independent copies remain separate definitions.
Outbound submission remains plain text and therefore does not claim live
harness enablement.

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
- `onboarding_state`: one versioned installation-local completion marker. A
  pristine schema starts eligible for version 1, while migration from every
  prior schema initializes version 1 as completed.
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

The application owns the exact first-run copy and its typed managed-Herdr or
standalone variant. It constructs every thought through the private
`InstructionalTextBuilder`, appending each reviewed shortcut literal and its
semantic range together before sealing an ordinary create action. No boundary
infers shortcut annotations from completed prose. Only a fresh interactive
launch supplies that candidate to the store. SQLite begins one immediate write transaction, reads and conditionally
advances the marker, creates the session, inserts all six ordinary thoughts,
and rebuilds its derived search row before commit. A completed marker creates
the requested session empty. Any failure rolls back the marker, session,
thoughts, and derived data together. JSON and other noninteractive paths use
ordinary session creation and neither seed nor advance the marker.

### Invariants

- Every thought belongs to exactly one session.
- Thought positions are unique within a live session and are normalized in one
  transaction after reorder.
- Operation sequences increase monotonically within a session.
- Undo and redo commit new current state and move the operation cursor
  atomically.
- A multi-thought mutation is stored as one ordered batch with one inverse, so
  delete, duplicate, collapse, cut, and submit-and-remove remain one undo step.
- Split, extract, and merge are board-history operations whose ordered batch
  combines exact content and annotation replacement with neighboring creation
  or recoverable deletion. The SQLite adapter applies the batch, truncates redo
  revisions for content-replaced thoughts, advances one sequence, moves one
  board cursor, and rebuilds FTS inside one transaction. Undo and redo apply the
  complete inverse or forward batch after restart.
- Soft deletion remains recoverable until explicit pruning.
- Only the holder of the session lease may mutate that session.
- All timestamps are stored as UTC integers and rendered in local time.
- Presentation annotations are sorted, non-overlapping UTF-8 byte ranges within
  canonical thought content. They never replace or truncate that content.
- Annotation validation, partition, extraction closure, concatenation shift,
  and editor-change rebasing share the domain annotation-range owner. Adjacent
  annotations are never coalesced merely because their provenance values match.

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

Schema version 11 adds the versioned `onboarding_state` marker while retaining
storage protocol 10 because the marker does not change ordinary stored board
data. Schema and storage protocol version 10 registered shortcut-emphasis
annotations as durable thought and revision metadata. Version 9 introduced
invocation references. The annotation and invocation migrations are
transactional protocol stamps because the annotation column and JSON envelope
already exist. The current storage protocol prevents an older writer from
interpreting an unknown annotation variant as compatible state.

Schema version 12 and storage protocol version 11 register the durable split,
extraction, and merge operation payloads, including exact content replacement
and state-checked recoverable deletion mutations. The metadata-only migration
is a compatibility boundary: a protocol 10 process must never be admitted as a
compatible owner after protocol 11 payloads may exist. The separate schema 11
onboarding migration remains protocol 10, and migration 12 preserves its
completed or eligible marker exactly.

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
- Exact initiating session, prior and target versions, and acknowledgement for
  a verified in-app release-highlight announcement. No highlight content or
  telemetry is stored.
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

Restart acceptance is not replacement evidence. After each peer accepts, the
coordinator performs a fresh bounded, cancellation-aware registry wait. A peer
converges only when the same session appears under a different instance ID,
the same installation identity, the exact target version, and a published
control endpoint. The endpoint is published only after board restoration. The
coordinator writes the initiating session's content-free pending announcement
only after every peer converges, then requests the initiating restart. Peer
failure creates no announcement and releases the initiating process without an
`exec` request. Initiating restart rejection atomically discards its exact
pending announcement and releases the preparation barrier. A delayed accepted
initiating resume retains the pending record and may show it later under the
exact target. `restart_needed` is cleared only when the
exact target announcement selects the initiating session after board
restoration and owner-control publication. Automatic presentation is installed
only when that atomic transition completes or the same exact transition had
already completed. Control unavailability, cache failure, and stale cache state
suppress presentation and emit closed finalization failure codes.

If replacement discovery itself fails after peer restart requests, the
coordinator releases the initiating process immediately, retains
`restart_needed`, and creates no announcement. It does not leave the initiating
board blocked until the prepare deadline.

The pending announcement write is part of initiating restart admission. If its
private atomic write fails, the coordinator releases the initiating process
without asking it to restart, records restart convergence as incomplete, and
shows no announcement. This keeps the old session usable and prevents a
successful-looking update path from losing its required durable target.

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

Concurrent replacements may all first observe `MigrationRequired`, release
their shared leases, and contend for the exclusive lease. The winner performs
the verified migration. A contender that reaches the unchanged bounded
exclusive timeout makes one nonblocking shared acquisition and reopens in
refuse-migration mode. It resumes only if the winner already established the
current schema. If migration is still required, or an exclusive owner is still
active, it returns the ordinary bounded `schema_busy` result. This follower
revalidation never weakens shared and exclusive compatibility.

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

### Packaged release-highlight projection

The root `release-highlights.json` is included in the Rust executable at build
time. Domain validation owns its exact schema, canonical stable versions,
ascending uniqueness, three to six item bound, text bounds, and skipped-version
selection. Startup composition reads only the private update cache after the
session board is restored. The UI receives either no automatic presentation or
validated groups paired with one exact durable announcement identity.

The release-highlight overlay reuses the canonical overlay geometry, close hit
target, theme, terminal-cell wrapping, and protected input boundary. Rendering,
row measurement, scrolling, resize clamping, and hit testing share one semantic
projection. Manual reopen uses only the exact installed group and emits no
acknowledgement effect. Automatic dismissal remains visible until the update
state adapter atomically acknowledges the matching record.

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

The application interaction state is the exhaustive terminal-independent set
`Board`, `Compose`, and `Edit { thought_id }`. The UI dispatcher selects one of
these modes before interpreting a normalized key. Board alone resolves printable
shortcuts. Compose and Edit resolve text through editor commands. Host focus is
never an application mode switch. Focus gain remains an integration refresh
signal. Focus loss may collapse only the UI-only, untouched Compose presentation
from `Editor` to `Prompt`; it does not reduce an application action. `AppState`
initializes an empty durable snapshot as Compose, while a nonempty snapshot keeps
the existing Board focus contract.

`Primary+Enter` and `Primary+Shift+Enter` normalize to distinct Submit and
SubmitKeep intentions before plain Enter handling. Board resolves those typed
intentions as invariant aliases of its configured submit-and-remove and
submit-and-keep commands, so selection and insertion-row behavior stay identical
to the configured character spellings. Edit routes them directly to its active
thought, while Compose remains unchanged. Plain Enter therefore remains an
editor newline or smart-list command. Crossterm unit contracts and real PTY
diagnostics verify the platform Primary event encodings. The command palette
remains the modifier-independent fallback.

Vertical board input uses one semantic modifier ladder for both arrow and
configured character spellings: plain input moves focus, Shift extends an
anchored range, and Primary+Shift reorders one thought. Other modifiers resolve
to the base focus intention. At the insertion row, range and reorder are
thought-only no-ops while focus retains the boundary policy. Input normalization
preserves otherwise unknown Primary character chords until the board keymap
can resolve the configured shifted range key, including when enhanced keyboard
reporting encodes the shifted character without a separate Shift flag. It must
not let arrow and `j`/`k`-style bindings acquire different intentions.

The normalized unmodified physical `Delete` key is an invariant Board spelling
of the configured delete command and therefore reaches the same typed action,
locks, operation, persistence, and undo path. Modified physical Delete retains
a distinct normalized value and has no Board command meaning. The configured
character remains remappable independently. `Backspace` has no Board delete
meaning. Compose and Edit interpret both Delete values as forward text deletion,
while query owners retain their existing local text-editing behavior.
Unmodified Space retains its own normalized identity until the active owner
handles it. Edit mode consults the canonical presentation projection before
ordinary character insertion. When that projection resolves the exact canonical
selection to one collapsed substitution, one placeholder-agnostic editor
command inserts an ASCII space at the selection start without replacing the
selection. Modified Space remains ordinary text input, and Board, Compose,
queries, overlays, and the session browser retain their existing Space behavior.
One typed non-text navigation decoder maps Up and Down to `k` and `j` in
list-only surfaces and maps Left, Down, Up, and Right to `h`, `j`, `k`, and `l`
in direction choosers. It ignores irrelevant modifiers symmetrically across
both spellings. Text-entry dispatch runs outside those decoders, so the four
letters remain content there. Modal navigation resolves before a configured
Board shortcut, with Escape retained as the unconditional Help close action.

`Primary+A` selects the entire current thought only in edit mode. `Primary+U`
deletes one newline-delimited logical line as a single undoable edit. Logical
line commands operate on the text model and are independent of visual wrapping.

`Primary+Shift+U` requests containing-sentence deletion when the
terminal reports the chord distinctly. The Rope editor is the canonical owner
of sentence ranges. It applies the documented UAX29-C3-2 paragraph profile,
uses the smart-list parser to protect recognized item prefixes, resolves cursor
or selection ownership in canonical UTF-8 byte space, merges deletion ranges,
and emits one `TextChangeSet`. The editor exposes the same ranges for the UI's
reveal-before-delete fold guard. Terminal translation, palette dispatch,
rendering, wrapping, and persistence never reconstruct sentence boundaries.
The complete profile and research record live in
[`docs/SENTENCE_DELETION.md`](../docs/SENTENCE_DELETION.md).

The default `smart_lists = true` setting maps an unselected editor `Enter` to a
terminal-independent smart-newline command. The editor recognizes only the
bounded CommonMark-style markers supported by the product contract, preserves
the document's local LF or CRLF convention, and reports continuation or marker
removal as one explicit text-change transaction. The UI flushes that transaction
as one persistent revision. Selection replacement uses the ordinary newline
command, bracketed paste remains a distinct exact payload, and the command
palette exposes the ordinary newline command directly. Terminal Tab and BackTab
normalize to distinct UI intentions. The application supplies the validated
`list_indent_width` and smart-list policy to explicit editor indent or outdent
commands, then flushes each command as one persistent revision. The editor
computes all touched logical-line replacements against the same before document
and reports one ordered `TextChangeSet`; a selection ending at column zero
excludes that following line. Recognized list indentation reuses exact existing
prefix bytes. Each level adds or removes exactly one configured space unit
regardless of marker width, while an established tab-indented prefix adds or
removes one tab. No parent-content offset participates in indentation or
outdent, and later ordered markers are never renumbered. Outside recognized list
context, Tab is exact space insertion and BackTab is a no-op. Palette commands
route through the same intentions for modifier-independent keyboard and mouse
access. An editor cursor and optional selection handed through Escape are
transiently available only to the next command-palette editor action on the
same unchanged thought. This preserves the editor position across the explicit
Edit-to-Board boundary required to reach the portable command fallback.

Bracketed paste is one payload and one undoable edit. When no thought is
selected, paste creates and focuses a new thought. The application never tries
to split a paste heuristically.

Compose sends every character, paste, annotated paste, clipboard result,
movement, selection, and supported composition intention through the existing
editor. A content-changing outcome is snapshotted once and passed to the
canonical `CreateThought` action with exact content and annotations. That action
allocates the first durable identity and sequence, records one board history
entry, and produces the existing persistence batch. Content-free editor events
produce no action. This avoids a create-then-revise gap and makes crash, retry,
restart, undo, and redo use the existing atomic operation contract.

Native clipboard reads are asynchronous UI intentions stored with a typed
initiating owner. Board results remain Board-owned, durable editor results must
still match the same `ThoughtId`, and Compose results must match both the
Compose owner and its lifecycle generation. Exiting or materializing Compose
advances that generation. Late success and failure results are removed and
discarded before paste dispatch, so completion cannot reinterpret an old editor
request under a newer interaction mode.

Board-mode printable keys always pass through the configured command map, even
when the insertion row or a durable blank has focus. One typed boundary
insertion policy owns both outer positions. The second blocked upward movement
at the first nonempty thought inserts at position zero, while the second blocked
downward movement at the end of a nonempty final thought appends. Both create a
durable blank and enter its editor through the canonical create action. Arrow
and configured previous or next spellings normalize to the same semantic
confirmation outside text modes and may be mixed. Repeated movement while the
new blank remains empty cannot create additional thoughts. On the insertion
row, two consecutive semantic base downward navigation commands perform the
same durable create-and-edit transition. Range, reorder, unrelated input,
pointer input, and mode changes clear confirmation. Unsupported modifiers that
normalize to base focus retain the same confirmation. Other edit boundaries use
the same navigation state machine.

Empty-board aftermath is reconciled by one typed policy owned beside
`InteractionMode`. Deliberate local removals request Compose after the mutation;
passive and external mutations request Preserve. Owner-control additions retain
an active Compose editor and its insertion order. Owner-control deletion of the
last durable thought resolves invalid durable focus to Board but never invents
Compose. Startup is the only automatic snapshot-derived entry, so discovery,
update checks, attachment scans, focus reports, and background completion cannot
steal input state.

Entering Compose after a deliberate local removal resets its UI presentation to
`Prompt`. In particular, accepted submit-and-remove shows `+ Start typing` only
after the matching receipt journal transition and canonical deletion are both
durable. It never allocates a replacement blank thought. Clicking the prompt or
an explicit Board insertion action changes only the presentation to `Editor`;
typing or paste may materialize directly from either presentation.

Board multi-selection is transient UI state with two explicit, non-overlapping
forms: an arbitrary identity set and an anchored contiguous range. A range
stores stable thought identities for its anchor and focused endpoint and derives
its selected identities from current live board order. Shifted vertical movement
and the remappable range latch update the endpoint without wrapping or addressing
the insertion row. Pointer extension resolves through the current layout
snapshot before entering edit mode. Bulk application actions continue to receive
only ordered thought identities and do not depend on terminal modifiers or this
UI selection representation.

The remappable `select_all` board command replaces the arbitrary set with every
live thought identity in current board order. Forwarded `Primary+A` resolves to
that board command, while edit mode retains the editor's complete-text action.
Whole-board palette submissions capture the live ordered identities directly;
direction choice and target revalidation neither reconstruct nor mutate the
visible selection.

The normalized paste payload carries exact text plus optional typed provenance.
Attachment annotations retain only presentation-safe metadata and byte ranges;
the absolute path remains the canonical text. Large-paste annotations retain
derived line and grapheme counts. Invocation-reference annotations retain only
a bounded display label over the canonical, self-contained collaborator
location. Shortcut emphasis retains only its closed semantic kind and exact
UTF-8 range. Each durable kind exhaustively selects substitution or inline-style
behavior in one UI-owned projection. Substitutions own lossless
canonical-to-visible mapping and atomic folded interaction. Inline styles copy
every canonical byte and contribute only semantic visible ranges. Board and
editor rendering, measurement, wrapping, cursor and selection mapping, and hit
testing consume that same projection. Clipboard, recovery, CLI, search,
submission, and integration boundaries continue to consume canonical content.
Edits rebase unaffected ranges and dissolve intersected ranges. Revisions
persist both sides of the annotation change so undo and redo remain restart-safe.
Exact selected-substitution activation and placeholder-aware Space resolve from
that same projection. They do not inspect a formatted label or reconstruct a
semantic range from visible text.

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
- Projects recognized coding agents from one bounded snapshot for inert
  invocation-picker references, without exposing raw topology or terminal
  metadata above the adapter.

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
reported state. For remove-after-success, SQLite commits the accepted terminal
journal row and the unchanged-source `BoardOperation` in one transaction. The
reducer stages that operation without changing the visible board and applies it
to state and undo history only after the matching ordered commit receipt. A
failed commit is retained on the bounded persistence lane for the ordinary
retry path, while the source lock, Edit owner, exact editor content, and cursor
remain unchanged. An ambiguous SQLite retry succeeds only when both the stored
terminal outcome and durable operation receipt exactly match the original
compound request. While the staged operation owns the next sequence, a typed UI
flush barrier prevents owner, submission, history, transfer, pointer, and quit
transitions from crossing an unflushed editor revision. That deletion remains
one undoable operation. Every failure preserves the thought.

Direct Edit submission first flushes the active editor revision and then enters
the existing durability-gated submission state machine for that one thought.
It uses the same attachment preflight, target discovery and revalidation,
redacted attempt reservation, sending compare-and-set, semantic adapter call,
receipt matching, atomic terminal journal and removal commit, and conditional
removal. Keep returns to the same Edit owner. Remove enters Compose only after
the matching ordered receipt applies the staged canonical delete operation.
Empty Compose never
creates an attempt, and missing or ambiguous targets retain the complete editor
draft while using the existing refresh or chooser.

Whole-board keep and remove actions reuse this exact request, journal, receipt,
and deletion path with every live source identity. Prompt assembly uses the same
canonical blank-line separator and target-aware shared-starter policy as an
ordinary multi-selection. Empty boards do not create an attempt, and several
eligible directions use the existing chooser without a confirmation step.

Submission attempts use a content-redacted SQLite journal. Proqi first reserves
every source thought in `prepared`, compare-and-sets the attempt to `sending`,
invokes Herdr once with no open database transaction, then compare-and-sets a
terminal result. Only one active attempt may reference a thought. Recovery
changes `prepared` to `cancelled` and `sending` to `outcome_unknown`; it never
automatically retries an ambiguous delivery. The journal stores ordered source
identities, their SHA-256 digests, one aggregate payload digest, and a target
identity fingerprint, never prompt content or raw pane and agent session
identifiers.

Attachment preflight precedes creation of this journal attempt. Successful
preflight preserves the same direct Herdr request and journal transitions.
External files remain caller-owned paths, so a file can disappear after the
final successful check and before the receiving agent opens it. Proqi does not
silently copy external files or rewrite canonical paths.

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

Control protocol version 7 is current. Version 2 introduced legacy durable
presentation annotations. Version 4 added session rename, owner synchronization,
exact editor replacement, and durable collapse state. An add mutation carrying
an invocation-reference annotation requires version 6, so an older active owner
cannot silently persist content while dropping mention metadata. Exact
replacement carries a typed `rev_` idempotency identity plus either
the caller's expected SHA-256 content digest or an explicit force intention and
enters the ordinary editor revision history. The owner rejects every mutation of a source thought while its
submission is in flight. Cross-session delivery inspects the source, commits an
idempotent destination creation through the version 7 purpose-specific
preservation request or an acquired
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
- Update lifecycle events contain closed schema stages, aggregate participant,
  restart, and replacement counts, stable failure stage and code pairs, and
  final convergence. They contain no durable distributed update phase record.
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

Shortcut-emphasis authority follows that boundary. Supported application, UI,
CLI, JSON, control, import, transfer, and agent-facing APIs cannot originate an
arbitrary semantic range. The serialized kind carries no `system_owned` claim,
signature, or key. A structurally valid range inserted directly into SQLite by
the same user may therefore load; unknown kinds, malformed ranges, and
unsupported storage protocols still fail closed.

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
cargo xtask quality
cargo xtask check
cargo xtask test
cargo xtask ci-linux
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
  Vue, Svelte, and Astro sources. Public documentation and prose, including the
  README and architecture and product contracts, are deliberately exempt.
  The same ceiling applies to test code. When inline tests begin to obscure an
  implementation, move them into an adjacent behavior-owned `tests` module;
  do not compress either production or test code merely to satisfy the limit.
- `architecture` verifies the inward dependency graph, canonical domain API,
  and ownership of SQLite, terminal, process, environment, and filesystem
  implementation dependencies. Its detector tests include accepted and
  rejected examples, and the scan fails if expected source layers are absent.
- `quality` runs formatting in check mode, Git whitespace validation for
  unstaged, staged, and committed HEAD content, Clippy for all targets and
  features, source limits, reviewed-snapshot policy, and documentation warnings
  without rerunning the deterministic test suite.
- `check` remains the canonical local and pre-push aggregate: it runs `quality`
  followed by `test`.
- `test` runs the deterministic unit, contract, and integration suites.
- `ci-linux` copies the current checkout without Git metadata or build output
  into an ephemeral `linux/amd64` Docker workspace and runs the Linux quality,
  test, MSRV, dependency, coverage, package, and Debian commands from CI in a
  pinned official Rust image. Only its reusable tool cache persists below
  ignored `target/`.
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
- `msrv` checks compilation with the declared minimum supported Rust version.
  Manifest, toolchain, packaging, CI, and release-boundary changes run the full
  MSRV suite. The complete suite also remains available as a manual diagnostic.
- `pty` runs terminal scenarios on each platform where the harness is supported.
- `coverage` publishes one report from Linux for relevant Rust and toolchain
  changes and enforces a 70 percent line threshold. Exclusions must be narrow
  and justified in configuration.
- `crate` owns the registry package and publication dry-run contract once.
- Native package jobs retain their installed-product and Debian boundaries
  without repeating the registry dry run.
- `security` runs dependency advisory, license, source, and policy checks.
- `check` is an aggregate job that succeeds only when every required job has
  succeeded or has been explicitly marked inapplicable.

A preflight job uses the xtask-owned classifier on the complete pull-request or
push diff before the matrix starts. When every changed path is ordinary
Markdown, CI runs one lightweight documentation gate for whitespace and
repository-owned public-asset contracts. Reviewed files under
`.github/release-notes/` are product inputs even though they are Markdown;
the Rust test, coverage, audit, PTY, package, and platform jobs are explicitly
skipped. Any non-Markdown path runs the distinct product boundaries. Coverage
runs only for relevant code changes, and the full MSRV suite runs only for its
classified compatibility boundaries. The aggregate `check` job verifies the
exact expected success-and-skip topology, so a path optimization cannot leave
the protected required check pending or turn an unexpectedly skipped product
job into success.

The aggregate `check` job is the stable branch-protection contract. Individual
jobs may evolve without repeatedly changing repository settings. Superseded
pull-request runs are cancelled, release runs are never cancelled, workflow
permissions use least privilege, and every third-party GitHub Action is pinned
to a full commit SHA with its human-readable version recorded in a comment.

Tests that require a real desktop clipboard, a specific terminal emulator, or a
live Herdr session are gated smoke tests. Their deterministic equivalents remain
required on every pull request. Live smoke tests remain explicit milestone or
diagnostic work and are not scheduled.

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

Release readiness is a deterministic property of checked-in inputs, never
commit prose. The Cargo version, matching reviewed notes, bounded release
highlights, clean worktree, exact main identity, and absent canonical tag form
the preparation contract. Routine local preparation validates only those cheap
inputs. Development, milestone, and diagnostic commands retain full checks,
PTY, coverage, audit, packaging, rehearsal, full MSRV, and Linux container
parity without making them prerequisites of metadata preparation.

For an exact release-ready main SHA, the candidate workflow runs alongside
ordinary CI and has no publication credentials. It builds only Apple silicon
macOS, Intel macOS, and x86-64 GNU Linux artifacts on native runners. Each native
binary is built once. The Linux binary is reused byte for byte in the Debian
package. The GNU/Linux candidate is built on Ubuntu 22.04, must not require a
glibc symbol newer than `GLIBC_2.35`, and is started from its final archive on
Ubuntu 22.04, Debian bookworm, and Ubuntu 24.04. One Linux job generates a union
third-party notice file for all targets, so Intel macOS never compiles the
packaging tool.

A reviewed pinned `cargo-dist` configuration or equivalent narrow Rust tool
stages archives containing one executable, MIT license, required notices, and
shell completions. Jobs create and verify SHA-256 manifests, SPDX JSON SBOMs,
and GitHub OIDC Sigstore provenance attestations. Every third-party Action is
pinned by full commit SHA and ordinary CI remains read-only.

The checked-in release manifest is packaged with the crate and embedded in the
binary. One shared xtask validator compares its exact versions with GitHub note
filenames and titles, the Cargo version, and the requested tag. Quality,
release planning, standalone packaging, and crate packaging all fail closed on
missing, corrupt, unreviewed, or mismatched highlights.

The candidate workflow creates a 30-day immutable artifact only after every
target, installed smoke, crate dry run, Debian package contract, checksum, SBOM,
attestation, formula, and manifest step succeeds. The Debian package reuses the
verified Linux archive executable byte for byte. The manifest separates public
release files from private crate and Debian evidence and binds the future tag,
source commit, source ref, build run and attempt, exact workflow, filenames, and
file digests. A protected stable tag remains the explicit publication authority.
Promotion requires the tag commit to be the exact prepared main SHA, one
successful aggregate main CI run for that SHA, and exactly one successful,
unexpired candidate for the same version and SHA. It downloads by exact run and
artifact identity, verifies the REST artifact digest before extraction, then
verifies every internal hash and candidate attestation. Missing, expired,
duplicate, mismatched, conflicting, or unattested candidates fail closed.
Promotion checks the immutable tag commit, not the moving main tip. A later
main commit therefore does not invalidate an authorized prepared candidate.
Promotion adds tag-bound attestations and publishes the same bytes. It never
rebuilds a successful native candidate. A manual candidate dispatch provides a
non-publishing recovery path at main or at the exact protected tag.
Release creation is idempotent for absent releases, empty or partially uploaded
matching drafts, complete drafts, and already published identical assets. The
workflow creates an empty verified draft, reconciles exact candidate bytes,
uploads only missing assets, then downloads and verifies the complete set before
registry publication. Duplicate, unexpected, conflicting, or incomplete public
assets fail closed. GitHub Release notes are the only changelog. The protected
release environment has no manual approval gate. Release runs are never
cancelled and existing assets for a version are immutable.

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

Pinned Linux QA tools are published separately from release artifacts. The
single repository identity is owned by `tools/ci-linux/image.json`. A dedicated
workflow builds amd64 and arm64 variants on matching native GitHub-hosted
runners, validates pull requests without pushing, and publishes only from
trusted main activity using the short-lived repository token. Content-derived,
run-qualified tags are checked for absence before publication and are never
overwritten. Digest-only consumption, registry provenance, and SBOMs make the
input explicit. Its registry-backed BuildKit cache is regenerable and untrusted.
Neither Proqi source nor release artifacts enter the tools image. Native smoke
and explicit amd64 parity are xtask diagnostics, not routine release preparation.

## Source organization

Start with one library crate plus thin binaries:

```text
Cargo.toml
src/
  lib.rs
  domain/          entities, values, invariants, operations
  application/     AppState, reducer, effects, SessionService
  ports/           Store, Editor, Clipboard, attachment ports, AgentGateway, runtime traits
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
