# Product Vision

Status: v0.1.0 product contract

Product name: Proqi

Command: `proqi`
Last updated: 2026-08-25

## Vision

Proqi is a terminal-native scratchpad for people who work with several
coding agents at once.

It replaces the plain text editor that sits beside an agent session. It gives
each agent session its own resumable board of follow-up prompts, fragments,
questions, and pasted context. These thoughts remain editable until the user
copies one or submits it to the working coding agent.

The product must feel as immediate as typing into an already open text file,
while adding only the structure that materially improves agent work:

- Independent, editable thoughts instead of one undifferentiated document.
- Session isolation for multiple simultaneous instances.
- Crash-safe local persistence and later resumption.
- One-action copy and cut into the native clipboard.
- Persistent undo for editing and structural actions.
- Complete keyboard and mouse control.
- Continuous reflow when its terminal pane is resized.

Native copy and paste remain the universal transfer mechanism. In supported
Herdr environments, direct submission to a verified adjacent agent is an
optional progressive enhancement.

## Product principles

### Capture must be frictionless

Pasting into the board creates a thought immediately. Creating, editing,
copying, cutting, and deleting thoughts should take one direct action whenever
the current mode makes that action unambiguous.

There is no save command. Content is saved automatically.

### Structure must earn its place

A thought has content, position, timestamps, revision history, and session
membership. It does not require a title, status, priority, category, or due
date.

Titles would force the user to describe a thought before using it. They add an
interaction without helping the primary workflow. A collapsed thought uses its
first visible lines as its preview. The data model may support optional titles
later, but the default interface does not show or request them.

### Order is spatial organization

A thought can be used out of sequence, revised repeatedly, or left untouched
for days. The product does not label thoughts as pending or done.

### Ephemeral means disposable, not volatile

Thoughts are session scoped, local, and easy to discard. They are still
persisted so that a closed terminal, crashed process, or restarted machine does
not destroy work.

The first version does not silently expire sessions. Users delete or prune
them deliberately. Configurable retention may be added later with recoverable
defaults.

### Every interaction has keyboard and mouse parity

The keyboard experience should feel closer to Superhuman than to a traditional
form. Common actions use direct, memorable shortcuts and never require a menu.

The mouse is equally supported. Every visible note and control has a complete
click target. Clicking content places focus where the user clicked. Scrolling,
text selection, and relevant drag interactions work without requiring keyboard
follow-up.

Neither input method is a secondary compatibility layer.

### The interface follows the pane

Agent panes are resized frequently. Resize is a normal interaction, not an edge
case. The board continuously recomputes wrapping, note height, scrolling, and
cursor placement whenever terminal dimensions change.

The selected thought, editing cursor, and relevant text must remain visible
through a resize. No restart, redraw command, or manual correction is required.

### Local first and quiet by default

The product works without an account. Ordinary capture, editing, persistence,
search, copy, submission, and JSON automation require no network service. Proqi
has no telemetry and never sends thought content, paths, identifiers, clipboard
data, or session state for analytics.

Every eligible interactive release startup performs a bounded, disableable
stable-release check against GitHub in the background. Concurrent startups are
coalesced into one request and one actionable prompt for the installation. The
check sends only a bounded Proqi name and version User-Agent when required by
GitHub. It is never implicit in debug or source builds, tests, JSON commands,
the Proqi skill, or other noninteractive paths.

Local diagnostics are structured, content-redacted, user-private, and bounded.
They record lifecycle, stable command outcomes, and durable submission-state
transitions without recording thought or clipboard content, session names,
workspace paths, pane identifiers, or raw external responses. Users can
explicitly collect retained events into a versioned local support bundle. Proqi
never uploads the bundle and never overwrites an existing output file.

## Core concepts

### Session

A session is one resumable scratch context. Each running application instance
owns one session. A session records:

- A stable ID.
- An optional user-defined name.
- Its original launch directory and most recent opening directory as navigation
  metadata.
- Creation, most recent opening, and last activity timestamps.
- Whether it is currently open.
- Optional last-known terminal integration and verified adjacent-agent context.
- Its thoughts and persistent operation history.

Directory metadata helps rank, recognize, and resume sessions. It does not make
all instances in the same directory share one session. Terminal pane IDs are
diagnostic context only because they are not durable across terminal restarts.

### Thought

A thought is one independently editable body of plain text. It can contain one
line, many paragraphs, code, logs, or arbitrary pasted context.

A thought has no required title. Its content is the object.

### Board

The board is the vertically ordered set of thoughts in a session. It is the
default screen and normally uses the entire terminal pane.

The reference layout is intentionally closer to a quiet editor than to a task
manager:

```text
 august-research                                      3 thoughts

   Update identity and summarize the relevant changes across all sessions.

 ─────────────────────────────────────────────────────────────

 ▌ Gut, nun möchte ich, dass du mir eine TLDR zusammenstellst,
 ▌ da alle Cloud- und Codex-Sessions davon betroffen sind.
 ▌
 ▌ Beschreibe kurz, welche neuen Tools vorhanden sind und welche
 ▌ Guidelines nun global zu befolgen sind.

 ─────────────────────────────────────────────────────────────

   I would like to build the following project. I think it should
   be an editor-like application or a TUI application similar to...
   24 more lines                                                   expand

 n new   y copy   x cut   space select   c collapse   s submit
```

The green focus gutter is the strongest routine visual element. Notes have no
heading row or decorative card chrome. Whole-thought controls can appear for
the focused or hovered thought without permanently consuming a row.

The board spends no permanent row on a repeated product header. The footer can
allocate up to five responsive bands: transient status, session name, thought
count with mode and durability, labeled actions, and only the verified agent
targets that currently exist. Empty optional bands consume no row. The session
name remains a visible rename target at every supported height and truncates
without covering status or board state. Narrow panes shorten secondary labels
before any two regions can collide.

### Revision and operation history

Text revisions preserve editing history within a thought. Structural operations
preserve creation, deletion, cutting, duplication, and reordering history
within a session. Both histories survive application restarts.

## Session lifecycle

The working command model is:

```text
proqi                       Start a new session
proqi -c                    Continue the latest inactive session for this directory
proqi -r                    Open the session picker
proqi -r <id-or-name>       Resume a specific session
proqi sessions              List and manage sessions
```

Starting a fresh session by default prevents two adjacent agent panes from
accidentally sharing thoughts.

Only one process may edit a session at a time. Attempting to resume an active
session produces a clear message and offers its identity. It never interleaves
writes silently. An exclusive lease is released on normal exit and safely
reclaimed after a crashed process.

On exit, the application shows the exact command that resumes the session.
Normal exit, terminal closure, process termination, and machine restart all
leave persisted content resumable.

Deleting a session initially moves it to a recoverable trash state. Permanent
pruning is a separate, explicit operation.

### Session browser

`proqi -r` opens a full-screen session browser designed for recognition rather
than requiring users to remember an ID or assign a title.

Each result shows:

- Its optional name, or a derived excerpt from its first useful thought.
- One or two short thought-content previews.
- Thought count and last activity time.
- The directory from which it was most recently opened.
- Its original directory in the detail view when that differs.
- Its last verified adjacent agent, direction, and name when an integration
  supplied that context.
- Whether it is active, resumable, recovered after a crash, or in trash.

Typing in the browser immediately searches optional names, paths, and thought
contents. Sessions associated with the current directory rank first without
hiding results from other locations. Recent results are grouped by useful time
ranges such as today, yesterday, the previous week, and older.

Arrow keys and `j` and `k` navigate. `Enter` resumes the selected session. A
mouse click on a resumable result performs the same action. Session management
actions such as rename and move to trash remain available without dominating
the browsing surface.

The browser is responsive. Wide panes use a result list beside a detail and
content-preview panel. Narrow panes use compact results and expand the selected
result inline. Every important field remains accessible in both layouts.

Active sessions remain visible but cannot be opened for concurrent editing.
Their active lease and last-known location are shown. A stale lease caused by a
crash is recovered before the session is marked resumable. The most recent
opening metadata changes only after the process successfully acquires the
session lease.

The browser stores `origin_cwd`, `last_opened_cwd`, `last_opened_at`, and
`last_active_at`. Optional integration context may include the provider and the
last verified adjacent agent kind, name, and direction. It must not include
conversation content, and resumption must never depend on a stale workspace,
tab, or pane identifier.

## Board behavior

### Default note presentation

Thoughts are titleless and expanded by default. Each thought takes exactly the
height required by its wrapped content, internal padding, and focus treatment.

Short and medium thoughts therefore remain fully readable in the board. This
preserves the useful quality of the current Sublime Text scratchpad, where the
next prompts can be read without opening them one by one.

Adjacent visible thoughts use a deliberate two-row cadence when the viewport
has room: one quiet horizontal rule followed by one blank row. Shallow panes
and the optional compact density use the rule alone. The rule belongs to the board
presentation, is never a card border or mouse target, and is allocated only
when the following thought can still receive a content row.

### Long thoughts

A thought collapses only when its natural height would dominate the useful
viewport. The cap is derived from the current terminal height, not from a fixed
number of lines.

A collapsed thought shows:

- Its leading content using the current wrap width.
- A quiet overflow indicator with the hidden line count.
- A direct keyboard and mouse action to expand it.

Expanding a long thought gives it the available board height without making
other thoughts unreachable. Editing it opens the full editor viewport with
internal scrolling. Mouse-wheel board scrolling advances one wrapped row at a
time before advancing to the next thought. It cannot overscroll an underfilled
board or hide the final insertion row.

Collapse is a presentation state. It never modifies content.

### Paste behavior

Bracketed paste is treated as one semantic input event.

- Pasting in board mode creates one new thought at the active insertion point,
  enters edit mode, and places the cursor after the pasted content.
- Pasting in edit mode inserts the complete payload at the cursor.
- When no thought is selected, `Ctrl+V` reads the native clipboard, creates one
  new thought containing its complete payload, enters edit mode, and places the
  cursor after the pasted content.
- Whitespace, blank lines, Unicode, and line endings are preserved.
- A large paste does not freeze rendering or briefly create one key event per
  character.
- Dragging one or more files into Proqi inserts their absolute paths. In edit
  mode they enter the current thought; in board mode they create and focus one
  thought. Unicode names, spaces, quotes, local file URLs, and POSIX shell
  escaping remain supported.
- When the native clipboard contains raw image pixels, `Ctrl+V` writes a private
  durable PNG inside the current Proqi session and inserts its absolute path.
  Proqi never uploads or analyzes the image automatically.

File paths and large pasted context use folded presentation immediately in both
board and edit mode while their canonical text remains exact. Images appear as
`[Image 1]`, other files as `[File 1]`, and context at or above 12 logical lines
or 1,200 perceived Unicode characters as
`[Pasted text · N lines · N characters]`. Thousands use comma grouping.
Numbering restarts for each thought. The complete bracketed token uses the
forest-green accent plus bold as a non-color cue, without exposing temporary
paths or filenames.

Moving onto or clicking a folded token selects its complete visible placeholder.
`Enter` expands the selected token's exact path or text for editing. Typing,
backspace, and delete replace or remove the complete canonical range. A
collapsed token is one logical editor unit, so the cursor can never disappear
inside hidden content.
Leaving edit mode returns expanded ranges to their compact presentation. Copy,
cut, export, search, recovery, and agent submission always use exact canonical
content regardless of the visible fold.

Explicit `http://` and `https://` URLs use the accent color and underline as a
visual link cue. Styling never changes the canonical content or terminal link
target.

Fold metadata is durable and participates in editor undo and redo. Editing
inside a folded range dissolves that range safely, while edits outside it
preserve and rebase its offsets. Restarting the application restores the same
folded presentation and undo state.

Creating and populating a thought through one paste is one undoable operation.
If clipboard access fails, `Ctrl+V` reports the failure without creating an
empty thought. Bracketed paste produces the same result without depending on a
terminal forwarding `Ctrl+V` as a paste shortcut.

Path conversion is deliberately conservative. Proqi converts a payload only
when every referenced item is an existing local file. Otherwise it preserves
the original text exactly. Ordinary dropped files stay in their original
location; only raw clipboard images are materialized into Proqi-managed storage.

Splitting one paste into several thoughts is an explicit later action, not an
automatic heuristic.

### Creation affordance

Every active insertion area exposes a quiet `+` control. It can remain visually
subtle until the area is focused or hovered, but it must always have a keyboard
equivalent and a stable mouse hit target.

Clicking `+`, clicking the insertion area, or pressing the create-thought key
creates one durable blank thought, enters edit mode immediately, and places the
cursor in it. The blank remains part of the session after `Esc`, exit, crash,
and resume. Creating it is an ordinary undoable board operation. The complete
insertion row is clickable and reads
`+ New thought` while focused or hovered instead of relying on an unexplained
symbol. The user never needs a second action before typing.

The insertion row is part of keyboard focus order. Moving down from the last
thought focuses `+ New thought`. `Enter` or `n` creates a durable blank. Moving
up or pressing `Esc` leaves the insertion row without creating anything. Two
consecutive downward navigation commands while the insertion row remains
focused create the blank and enter its editor, including on an empty board.
When an editor is already at the final thought, the second consecutive blocked
downward movement creates the blank and enters its editor directly.

When the board contains no thoughts, the insertion row owns keyboard focus by
default. Printable keys remain board commands on the insertion row and on a
focused durable blank. Typing content always requires the explicit create or
edit transition first. This keeps delete, command discovery, navigation, help,
submission, and every configurable board shortcut reachable in those states.
Bracketed board paste remains the deliberate one-action create-and-paste path.

### Copy, cut, and delete

Copying a thought writes its exact content to the native system clipboard.

Cut is atomic from the user's perspective. The product writes the content to
the clipboard first and removes the thought only after clipboard success. A
clipboard failure leaves the thought unchanged and displays an actionable
error.

Deleting removes a thought without changing the clipboard. Both cut and delete
can be undone after restarting the application.

### Multi-selection

`Space` toggles the focused thought in a visible board selection. Selected
thoughts retain their board positions and receive the same non-color focus cue
as the active thought. Copy, cut, delete, duplicate, collapse, and adjacent-agent
submission address the selected set in board order. Each structural action is
one persistent board operation and therefore one undo step. Reordering remains
a single-thought action.

`Meta+D` duplicates the focused thought or complete selection. Exact content,
annotations, and presentation preferences are copied in board order directly
below the source range. Duplicates receive fresh identities and timestamps,
become the new selection, and are created as one persistent undo step. Entering
edit mode or pressing `Escape` clears the complete board selection.

Copy and submission concatenate exact thought content with one blank line
between thoughts and no generated labels. A multi-thought Herdr submission is
one semantic prompt request, not several deliveries. For submit and remove, all
unchanged source thoughts are removed together only after the matching accepted
receipt is durably journaled. Every source thought is locked against TUI and CLI
mutation from submission intent until the attempt reaches a terminal journaled
state.

### Submit to an adjacent agent

When Proqi runs beside one or more agent panes in a supported terminal
environment, it can submit the focused thought directly to a verified adjacent
agent above, below, left, or right.

This is a progressive enhancement. Copy and cut remain available in every
terminal. Submission controls appear only when an installed integration can
identify an eligible adjacent agent with confidence.

Herdr is the first supported integration. It provides directional pane lookup,
agent detection, optional session identity, readiness state, and an agent-aware
prompt operation. Proqi uses that semantic operation instead of simulating
arbitrary terminal keystrokes.

A submission target is eligible only when all of the following are true:

- The target pane differs from the Proqi pane.
- Both panes belong to the same tab and workspace.
- Their terminal geometry confirms that the target is in the requested
  direction and overlaps the relevant edge.
- The target is recognized as a supported interactive agent.
- The target exposes enough identity to show the user where the thought will
  go.

Directional lookup is never trusted without these independent checks. The
product never guesses a target and never falls back to raw input injection.
Otherwise eligible sessionless Codex and Cline panes are the explicit
provisional exceptions. Proqi revalidates the same empty pane and harness kind
immediately before delivery and accepts only a matching receipt. Because Herdr
can acknowledge the prompt before a session hook reports the new identity, a
still-provisional receipt triggers immediate rediscovery without resending. A
later established session replaces the provisional binding and must match
exactly thereafter. Other sessionless agent kinds remain ineligible.

Prompt delivery has two dispositions over the same immediate semantic submit
operation. `Submit` is the default and deletes the source thought or selected
thoughts only after an accepted matching receipt. `Submit & keep` sends the
same prompt and retains the source. Both may submit while the agent is working.
The receiving harness decides whether the input steers the current turn or
becomes follow-up input. Proqi verifies at submission time that the target
exposes Herdr's semantic request and receipt contract. It never substitutes raw
key injection.

Herdr protocol 19 does not guarantee a distinct user-turn boundary when another
sender submits concurrently. Overlapping inputs can therefore merge at the
receiving harness even though Herdr returns an accepted receipt. Proqi treats
this as a known integration limitation and preserves its ordinary verified
submission workflow. Protocol 19 also has no stable pre-session agent-instance
identity or atomic expected-instance precondition. Replacing one sessionless
Codex or Cline process with another of the same kind in the same pane during the
narrow interval between revalidation and delivery is therefore not detectable
by Proqi.

Each verified adjacent target appears once in the integration row, without its
readiness label. `s Submit` and `S Submit & keep` are shown only when
semantic submission is available. If exactly one eligible target supports an
action, that action is direct. If several support it, delivery enters a
directional targeting state.
Arrow keys and `h`, `j`, `k`, and `l` choose among the enabled directions. Mouse
users select the corresponding adjacent-agent indicator.

In a Herdr-managed pane, Proqi publishes the display-only pane label `proqi`
with a short lease and clears it on normal exit. This helps users distinguish
the scratchpad beside several named agent panes. It never claims an agent
identity, and stale display metadata expires after a crash.

Submit and keep always preserves the thought. Submit and remove deletes it only
after the integration returns an accepted receipt for the exact request, and
that deletion remains undoable. A failed, timed-out, ambiguous, unsupported, or
mismatched submission leaves the thought unchanged and reports that it was kept.

Submission does not wait for the agent's response and does not import or inspect
the agent conversation. The receiving harness decides whether a prompt sent
while it is working is queued, treated as steering, or rejected. Proqi shows
the agent state but does not reinterpret the harness's behavior.

### Reordering

Thoughts can be moved up and down with direct keys and mouse drag. Reordering is
immediate, autosaved, and undoable.

Keyboard reordering wraps across the board boundaries. Moving the last thought
down places it first, and moving the first thought up places it last. Mouse drag
remains positional and does not wrap.

## Interaction model

### Board mode

Board mode optimizes browsing and whole-thought actions. Suggested default
bindings are:

| Action | Keyboard | Mouse |
|---|---|---|
| Create thought | `n` | Click `+` or the insertion area |
| Paste as new thought when none is selected | `Ctrl+V` or native paste | Use the terminal paste action |
| Edit thought | `Enter` or `e` | Click at the desired text position |
| Copy thought | `y` | Click copy control |
| Cut thought | `x` | Click cut control |
| Delete thought | `d` | Click delete control |
| Duplicate thought or selection | `Meta+D` | Command palette |
| Select or deselect thought | `Space` | Click the thought, then use the selection control |
| Submit and remove after acceptance | `s`, when supported, then direction when needed | Click verified Submit control |
| Submit and keep thought | `S`, when supported, then direction when needed | Click verified Submit & keep control |
| Undo board action | `u` | Click undo control when visible |
| Move thought | `J` and `K`, or `Shift+↑` and `Shift+↓` | Drag thought handle |
| Expand or collapse | `c` | Click overflow indicator |
| Search | `/` | Click search control |
| Help | `?` | Click help control |
| Exit | `q` | Click exit control |

Final bindings remain configurable. The product must not depend on terminals
forwarding `Cmd+C`, `Cmd+V`, or Meta keys consistently.

### Meta and primary shortcuts

Proqi supports familiar modifier shortcuts when the terminal reports them. In
the user-facing keymap, `Meta` means the platform's primary application
modifier: Command on macOS and Control on Linux. Internally this is
normalized as `Primary`, independently of the terminal's raw modifier name.

Initial editing shortcuts include:

| Action | Preferred | Portable fallback |
|---|---|---|
| Select all text in the focused thought | `Meta+A` | `Ctrl+A` or command palette |
| Delete the current logical line | `Meta+U` | `Ctrl+U` or command palette |

Select all is scoped to the current thought in edit mode. It does not select
all thoughts in board mode. Delete line removes one newline-delimited logical
line, not only the currently wrapped visual row, and is one undoable edit.

Many terminals consume Command shortcuts before a TUI can receive them. Proqi
therefore supports enhanced keyboard protocols where available, configurable
bindings, and portable fallbacks. Core functionality never depends on a
terminal forwarding Command or Meta successfully.

### Edit mode

Edit mode behaves like a focused multiline text editor. It supports:

- Character, word, visual line, and document movement.
- Text selection by keyboard and mouse drag.
- Native clipboard copy, cut, and paste.
- Insert, replace, delete, undo, and redo.
- Horizontal content represented through wrapping, not a hidden horizontal
  scroll mode by default.
- Optional external editor handoff later.

Leaving edit mode returns to the same board position and keeps the edited
thought selected.

At the first or last visual line, the first blocked vertical arrow confirms the
boundary. Repeating the same blocked movement leaves edit mode and focuses the
adjacent thought. Any other input resets the confirmation. This behavior has no
timer, and plain `j` and `k` remain editable characters in text contexts.

### Mouse interaction

Mouse support includes:

- Single click focus on every visible note and control.
- Cursor placement at the clicked text cell.
- Drag selection inside text.
- Scroll wheel and trackpad scrolling.
- Drag reordering through a dedicated handle or gutter.
- Clickable `+` controls at active thought insertion points.
- Clickable overflow, search, help, undo, and session controls.
- Clickable verified adjacent-agent targets when an integration is available.
- Hover treatment when the terminal reports mouse motion.

The active gutter centers a vertical-ellipsis drag affordance at every thought
height. During a drag,
the existing separator at the proposed destination changes to the accent color
without reflowing the board. Proqi does not use timer-driven decorative
animation. Terminal motion is reserved for immediate interaction feedback.

Right click is never required because terminals reserve or forward it
inconsistently. All mouse actions have keyboard equivalents.

### Cross-session thought delivery

The command palette can copy the selected thought into another resumable Proqi
session. A searchable destination picker matches exact names, paths, and
derived excerpts. Duplicate names remain valid, but a typed session identifier
is required when a name is ambiguous.

Send preserves the source. Send and remove commits the exact content and its
presentation annotations in the destination first, then performs an ordinary
undoable source deletion only after the destination returns a durable receipt.
Failure, ambiguity, or an unsupported active owner leaves the source unchanged.
Undo restores only the source deletion and never retracts the destination copy.
The scriptable CLI exposes the same behavior with `thoughts send` and separate
idempotency identifiers for destination creation and optional source removal.

### Interaction economy

The following actions must not open a confirmation dialog:

- Create thought.
- Edit thought.
- Copy thought.
- Cut or delete a thought when undo remains available.
- Submit to the only eligible adjacent agent.
- Submit and remove when deletion remains undoable.
- Reorder thought.
- Collapse or expand thought.
- Exit after successful autosave.

Confirmation is reserved for irreversible pruning, destructive recovery
choices, and operations that affect more content than their immediate target
implies.

## Updates

Proqi supports only the latest stable release during the `0.x` series. Update
checks consider stable GitHub Releases only. Drafts, prereleases, malformed
tags, and older or equal versions never produce a prompt.

Interactive release builds enable `check_for_updates = true` by default. The
setting can be disabled globally. Check results, `Not now`, and `Skip this
version` are installation-wide so 10 to 15 adjacent Proqi processes do not each
contact GitHub or compete for attention. `Not now` defers the release until the
next successful eligible startup check. `Skip this version` remains exact and
durable until a later release exists. A shared private cache stores only the
latest stable version, refresh generation, last successful check time, exact
dismissed or skipped version, observed installed version, restart-needed state,
and bounded HTTP cache metadata. Cache corruption is a miss and never blocks
startup.

At most one process refreshes the generation observed by a concurrent startup
cohort, and at most one process owns the actionable prompt. A later independent
startup checks again. Other sessions continue normally. The command palette
offers an explicit `Check for updates` action. JSON commands, the Proqi skill,
and noninteractive commands never check unless the user explicitly runs
`proqi update check --json`.

### Homebrew update and restart

The supported automatic action is available only for a verified installation
from the `oborchers/tap/proqi` Homebrew formula on macOS or Linux. The prompt
offers:

- `Update and restart all sessions`.
- `Not now`.
- `Skip this version`.

The prompt shows the verified count of affected active sessions and has full
keyboard and mouse operation. Before installation, one elected coordinator
asks every live Proqi process from the same installation and compatibility
domain to flush durable work and acknowledge readiness. A save failure,
negative acknowledgement, verified live timeout, or lost coordinator cancels
the operation before Homebrew runs and returns every participant to ordinary
use.

After all participants are ready, Proqi runs exactly one direct process with no
shell interpolation:

```text
brew upgrade --formula oborchers/tap/proqi
```

If Homebrew fails, every old process remains usable and no restart is attempted.
After success, the coordinator rescans active instances. Each macOS or Linux
participant completes durable flushing, terminal restoration, lease release,
and resource cleanup, then uses Unix process replacement to resume the same
session in the existing pane. A failed replacement never rolls back successful
peers. It is reported truthfully, leaves that session resumable, and offers a
direct retry. Proqi never claims that all sessions restarted while an old
process remains.

Existing shared schema leases remain the compatibility barrier. A new process
does not migrate while an old process still holds a conflicting lease. It waits
for bounded restart convergence or reports that restart remains pending.

### Archive, Debian, Cargo, and unknown installations

Standalone archive users receive the verified release URL and external
replacement instructions. Proqi does not overwrite a standalone executable and
does not promise same-pane restart for archive installations. Durable sessions
resume on the next normal start after the user replaces the binary.

Debian package and Cargo installations never invoke their package manager from
Proqi and do not receive an implicit package-manager update. Debian users
download and verify the newest `proqi_amd64.deb`, then install that local file
again. Cargo users rerun the documented Cargo installation command. Source and
other unknown installations receive accurate non-destructive guidance or no
action.

No installer action is automatic. Update installation always requires an
explicit user choice. A future standalone updater may implement a separately
reviewed replacement boundary, but it is outside `v0.1.0`.

## Responsive terminal layout

The application listens to terminal resize events and reflows on every size
change.

### Resize invariants

After any resize:

- All text wraps to the new content width.
- Every natural and capped note height is recomputed.
- The focused thought remains visible.
- The editing cursor remains visible and refers to the same text position.
- A mouse hit target always matches its newly rendered geometry.
- Scroll bounds and scrollbars reflect the new layout immediately.
- Underfilled content cannot be scrolled away from the viewport.
- The final scroll page reserves enough space for `+ New thought` above the
  footer, including when every thought overflows the initial viewport.
- No content is clipped behind the footer or terminal edge.
- Undo history and presentation state remain unchanged.

### Height allocation

The board first reserves only the minimum rows needed for global status and
contextual shortcuts. The remaining rows belong to thoughts.

For each thought, the renderer calculates its natural height from its wrapped
visual lines. A viewport-aware cap is then applied only to long thoughts. As
the pane becomes taller, more of a long thought becomes visible. As it becomes
shorter, long thoughts collapse further before short thoughts lose their
natural height.

The focused thought receives enough height to keep its active content visible,
subject to the minimum space required to navigate the rest of the board.

### Width adaptation

The layout remains one column at every supported width.

As width decreases, the interface progressively removes nonessential chrome:

1. Secondary timestamps and verbose hints disappear.
2. Padding narrows.
3. Full borders become a compact focus gutter.
4. Labels become symbols only when the symbol is unambiguous and help remains
   available.

Content and core actions never disappear. There is no fixed desktop layout
that merely clips in a narrow pane.

### Resize quality requirements

Resize handling must be tested through repeated, rapid changes rather than one
final dimensions event. Rendering should coalesce redundant events while still
tracking the latest size. It must not flicker, drift, lose the alternate screen,
or require a manual redraw command.

Tests must cover narrow side panes, tall panes, shallow panes, tmux splits,
window snapping, and rapid drag resizing in common macOS terminal emulators.

## Visual design

The interface uses a deliberately small subset of Oliver Borchers' canonical
brand palette. The website vocabulary is not copied literally into the
terminal. Glass, gradients, glow meshes, technical grids, animation, and visual
texture would add noise and render inconsistently across terminals.

### Core palette

| Role | Light | Dark |
|---|---|---|
| Background | `#FAFAF8` | `#0F0D0A` |
| Primary text | `#1E1B18` | `#E8E4DF` |
| Accent text | `#2D6A4F` | `#70D69B` |
| Accent surface | `#2D6A4F` | `#2D6A4F` |
| Focused surface | `#ECECF0` | `#34343F` |
| Quiet border | `#E0D9CF` | `#2A2520` |
| Muted text | `#4F463E` | `#B0A9A0` |

The terminal may inherit its existing background where exact background color
control would make integration feel less native. Accent and focus treatment
remain brand-derived.

Forest green remains the only routine hue. A bright mint expression of that hue
marks folded tokens and actionable text on dark backgrounds. The deeper forest
surface marks the focus gutter with a contrasting foreground.
Focused thought text remains in the primary foreground on a quiet neutral
surface. Muted text and borders establish hierarchy without introducing more
hues.

Semantic error, warning, and information colors appear only when those states
actually exist. They are not decorative note colors.

### Brand expression in the terminal

The selected thought uses a one-cell forest green edge or gutter and the quiet
focused surface from the core palette. This is the terminal translation of the
canonical brand edge without turning the complete selection into an accent
block. Unselected thoughts use the quiet border or whitespace alone.

Edit mode requests a blinking block cursor where the terminal supports cursor
styling safely. The terminal owns its blink timing; Proqi does not paint a
permanent cursor cell that could mask the blink. There is no glow or motion
beyond the functional terminal cursor.

The interface relies on spacing, wrapping, and contrast before borders. A
thought should look like readable text with focus, not like a dashboard card.

### Theme behavior

The default theme is `auto`, with explicit `light`, `dark`, and `limited`
overrides. Users may replace any semantic color through inline configuration or
a bounded local TOML theme file. Relative theme paths resolve from the Proqi
configuration directory, and inline overrides take precedence over file roles.
The current adaptive Proqi palette remains the exact default.
Automatic detection uses terminal capabilities where available and falls back
to terminal-native foreground and background colors rather than guessing an
unsafe contrast pair.

In automatic mode Proqi queries the terminal's default foreground and
background before terminal input begins. The focused surface is a quiet neutral
blend derived from that background while primary text keeps the terminal's
exact foreground. The same semantic focus surface is used for thoughts,
insertion rows, session-name affordances, footer controls, modal selections,
and hover states. A failed query retains terminal-native colors plus the
non-color focus gutter.

Custom themes use versioned schemas and semantic roles for foreground,
background, accents, focus, links, annotations, statuses, dividers, and errors.
They are loaded only at launch. Proqi does not download themes or maintain a
theme registry. Custom palettes that fail required text, focus, accent-surface,
or control contrast are rejected with the failing role pair rather than
silently altered.

The product remains usable in terminals without true color. The fallback uses
default foreground and background, one supported green accent, bold, dim, and
reverse video sparingly.

## Accessibility and input correctness

- All functionality is available without a mouse.
- All visible actions are operable with a mouse.
- Focus is always visible through more than color alone.
- Color is never the only carrier of status.
- The application respects reduced motion by avoiding nonessential animation.
- Unicode behavior is tested with umlauts, combining characters, emoji, CJK,
  right-to-left samples, and wide terminal cells.
- Keybindings are remappable for terminal, keyboard layout, and accessibility
  differences.
- Help is available in context and searchable through a command palette.

## Undo and recovery

There are two explicit undo contexts:

- Editor undo restores coalesced text edits in the active thought.
- Board undo restores structural operations such as create, cut, delete,
  duplicate, and reorder.

Undo history is persisted with the session. Restarting the process does not
turn a reversible deletion into permanent data loss.

Autosave uses short, atomic transactions. A storage failure is visible and does
not masquerade as a successful save. The application continues to hold the
unsaved buffer in memory while presenting recovery choices.

Crash recovery tests include process termination during typing, during a large
paste, during reordering, and while the database is temporarily busy.

## Technical direction

The recommended implementation is a Rust application distributed as native
binaries.

Initial components:

- Ratatui for layout and rendering.
- Crossterm for terminal events, resize, mouse input, and bracketed paste.
- A Ropey-backed multiline editor behind a terminal-independent abstraction.
- SQLite through `rusqlite` with bundled SQLite.
- `clap` for commands and session flags.
- `arboard` for local clipboard access.
- OSC 52 as a terminal clipboard fallback where supported.
- `serde` and TOML for configuration.
- `tracing` for typed, structured, bounded file diagnostics.

The editor, clipboard, storage, and rendering layers remain behind internal
interfaces so their implementations can change without changing the product
model.

### Integration boundary

The core defines a narrow adjacent-agent interface for capability discovery,
verified target resolution, prompt submission, and structured errors. The TUI
does not contain Herdr-specific process or protocol logic.

The first adapter invokes the installed Herdr CLI directly without a shell. It
uses structured JSON responses, verifies the client and server protocol, applies
bounded timeouts, and treats integration failures as non-destructive. Thought
content is never interpolated into a shell command.

The adapter resolves candidates in all four directions and then validates their
pane identity, tab and workspace membership, geometry, detected agent, session,
and interactive state. A direction with no verified candidate is unavailable.
The adapter does not search unrelated tabs or workspaces for a convenient agent.

Other multiplexers may implement the same interface later. Unsupported
terminals expose no submission capability and retain the complete clipboard-first
workflow.

### Dedicated Proqi skill

The public project ships a dedicated `proqi` skill for supported coding-agent
harnesses. Depending on the harness, users invoke it as `/proqi`, `$proqi`, or
by naming the skill in natural language.

The skill teaches an agent to discover the installed Proqi version's
capabilities and use its current versioned JSON CLI. It does not duplicate
application logic. It supports explicit
user requests to list or find sessions, inspect a specified thought, add a
thought from standard input, and perform reversible thought operations.

Agent-facing commands provide versioned JSON, opaque IDs, structured errors,
standard-input support for arbitrary text, and operation receipts. Mutations to
an active session are forwarded to its owning Proqi process so they pass through
the same reducer, lease, persistence, and undo rules as TUI actions.
Exact replacement accepts a typed revision identifier so a retry can resolve to
the original durable editor revision without applying the content twice.

The skill never reads all scratchpad content merely because it is installed. It
acts only when invoked, addresses the session requested by the user, and does
not transmit content to a model provider beyond the agent interaction the user
already initiated.

### Persistence and concurrency

The canonical store contains sessions, thoughts, thought revisions, structural
operations, session leases, and schema migrations.

SQLite uses short transactions, WAL mode, a busy timeout, and bounded retry.
Multiple application instances are a normal tested condition. Storage errors
are never silently swallowed.

An exclusive cross-process lease prevents two instances from editing the same
session. Different sessions can remain active concurrently.

## Open source direction

Proqi is an Oliver Borchers personal open source project licensed under MIT.
Contributions use the same MIT terms as the repository. No contributor license
agreement or Developer Certificate of Origin sign-off is required.

Public collaboration uses GitHub Issues and pull requests. GitHub Discussions,
a support mailbox, and an in-repository changelog are not part of the project.
GitHub Releases are the only release changelog. Security reports use GitHub's
private vulnerability reporting and the security policy supports only the
latest stable release. Contributor behavior follows Contributor Covenant 2.1.

The minimum supported Rust version for `0.x` is 1.88. The public CLI, config,
and JSON surface may change before `1.0`; machine JSON remains explicitly
versioned. Database integrity, forward migration, backup, newer-schema refusal,
typed identifiers, and mixed-version safety are mandatory regardless of the
pre-`1.0` compatibility policy.

The first public version is `v0.1.0`. Release targets are:

- Apple silicon macOS.
- Intel macOS.
- x86-64 Linux using GNU libc 2.35 or newer.

Distribution is limited to immutable GitHub Release archives and the
`oborchers/homebrew-tap` personal tap. The tap provides one prebuilt Homebrew
formula, installed with:

```text
brew install oborchers/tap/proqi
```

There is no crates.io, npm, PyPI, Docker, Homebrew Core, shell installer, or
binary cask in `v0.1.0`. The prepared next release adds a crates.io binary
package and one x86-64 Debian package without adding an APT repository. The
crate requires Rust 1.88 or newer and does not establish a supported Rust
library API. The Debian asset is installed as a verified local file and
preserves user state on removal. Release archives contain the
native executable, MIT license, required notices, and shell completions.
Published artifacts also receive SHA-256 checksums, SPDX JSON SBOMs, and GitHub
OIDC Sigstore build-provenance attestations. Paid Apple signing and notarization
are not used.

Pull requests and `main` use one aggregate CI contract. Direct owner pushes
remain allowed, force pushes do not. Immutable `vX.Y.Z` tags start a protected
release workflow that builds, verifies, and publishes a GitHub Release.
Creating an allowed stable tag is Oliver's explicit authorization to publish
that release. Stable-tag protection allows only Oliver's individual bypass, so
no second environment approval duplicates that authorization. No package, tag,
tap, repository visibility, or GitHub setting is changed without Oliver's
explicit approval.

## Research and clean-room boundary

The project may study publicly documented behavior and openly licensed source
code from terminal agent harnesses and TUI libraries.

It must not use leaked source maps, decompiled proprietary bundles, or copied
proprietary implementation code as development material. Public availability
of an artifact does not grant an open source license. Using such material would
create provenance and licensing risk for contributors and downstream users.

Claude Code can be studied through its official documentation, public command
behavior, and independently written black-box compatibility tests. Codex,
Hermes Agent, Ratatui, Ink, Bubble Tea, and other openly licensed projects can
be studied within their licenses.

Research notes should record the public source and license for every borrowed
idea or implementation dependency. Product behavior inspired by another tool
must be implemented independently.

### Public research foundation

The current direction is grounded in these public primary sources:

- [Codex open source repository](https://github.com/openai/codex), including
  its Rust, Ratatui, and Crossterm TUI implementation.
- [Hermes Agent TUI documentation](https://github.com/NousResearch/hermes-agent/blob/main/ui-tui/README.md),
  including its session, clipboard, queue, and terminal interaction patterns.
- [Claude Code session documentation](https://code.claude.com/docs/en/sessions),
  including session creation, continuation, resumption, and local persistence.
- [Ratatui](https://github.com/ratatui/ratatui) and
  [tui-textarea](https://github.com/rhysd/tui-textarea) for openly licensed TUI
  and editor primitives.
- [SQLite WAL documentation](https://www.sqlite.org/wal.html) for local
  persistence and concurrent instance behavior.
- [Homebrew's Formula Cookbook](https://docs.brew.sh/Formula-Cookbook) and
  [dist](https://github.com/axodotdev/cargo-dist) for packaging and release
  direction.

## Deliberately out of scope for the first version

- Sending to unverified panes, unrelated tabs, or arbitrary terminal processes.
- Raw keystroke injection as a substitute for an agent-aware prompt operation.
- Direct Codex, Claude Code, Hermes, or model-provider API integrations. The
  first version integrates through Herdr's local semantic CLI only.
- Cloud accounts, synchronization, or collaboration.
- Shared live editing of one session.
- AI generation, rewriting, ranking, or automatic prompt organization.
- Required note titles, statuses, priorities, tags, or due dates.
- Rich text, rendered Markdown editing, or embedded media.
- Mobile or graphical desktop applications.
- Plugin systems.
- Automatic splitting of pasted text into several thoughts.
- Silent retention expiry.
- A background app server or public local service.
- A native IDE or editor extension.
- Native terminal scrollback as the primary board renderer.
- JSONL conversation history or agent-runtime storage.
- Bazel, JavaScript package wrappers, or multi-language launchers.
- Telemetry, update-check analytics, installation identifiers, or usage events.
- Automatic update installation without explicit confirmation.
- Standalone executable self-replacement in `v0.1.0`.
- Public repository changes, tap creation, credentials, or tags without
  Oliver's explicit approval. Creating an allowed stable tag explicitly
  approves the corresponding release publication.

## Later opportunities

These remain compatible with the vision but are not initial requirements:

- Merge and split thought operations.
- External editor handoff through `$VISUAL` or `$EDITOR`.
- Import and export as plain text, Markdown, or JSON.
- Configurable retention and recoverable pruning.
- Session handoff between machines without making cloud sync mandatory.
- Additional multiplexer and harness adapters that remain separate from the
  clipboard-first core.
- A library API for third-party frontends.

## Success criteria for `v0.1.0`

`v0.1.0` succeeds when a user can keep several instances beside
several agent sessions for a full working day and stop using a separate text
editor for prompt scratchpads.

Specifically:

- Starting a fresh board takes one command.
- A previous session can be found by its thought contents, last-opened path, or
  recency without remembering a title or ID.
- The session browser remains usable in narrow and wide panes and never allows
  two processes to edit one session silently.
- Pasting a thought takes one native paste action.
- Pasting a clipboard image or dropping local files inserts durable, usable
  paths in one undoable action without inspecting their contents.
- Copying or cutting a thought takes one direct action.
- When a verified Herdr agent is adjacent, submitting a thought takes one direct
  action for a single target or one action plus a direction for several targets.
- Submission can target verified agents above, below, left, or right without
  confusing their sessions.
- Failed or ambiguous submissions never remove or modify the thought.
- Notes remain readable through continuous pane resizing.
- Thoughts can be reordered by keyboard or direct mouse drag, and the new order
  remains durable and undoable.
- Keyboard-only and mouse-only workflows both complete every core task.
- An explicitly invoked Proqi skill can discover capabilities and add a thought
  to a specified active or inactive session through the supported CLI contract.
- A process or machine crash does not lose committed thoughts.
- Deleted thoughts and text edits can be undone after restarting.
- Simultaneous instances do not mix sessions or silently lose writes.
- Installation through Homebrew requires no language runtime setup.
- One installation-wide update request and one prompt serve 10 to 15
  simultaneous startups without transmitting user content, while a later
  startup checks again.
- A confirmed Homebrew update either checkpoints every verified participant
  before one installer runs or aborts before installation.
- Successful Homebrew updates resume macOS and Linux sessions through ordinary
  durable state and same-pane Unix process replacement, with partial failures
  reported accurately.
- Standalone archives provide external replacement guidance and next-start
  resume without claiming automatic self-replacement.
- The interface remains visually quiet after hours of continuous use.
