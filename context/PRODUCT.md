# Product Vision

Status: Working product vision

Product name: Proqi

Command: `proqi`
Last updated: 2026-08-24

## Vision

Proqi is a terminal-native scratchpad for people who work with several
coding agents at once.

It replaces the plain text editor that sits beside an agent session. It gives
each agent session its own resumable board of thoughts, prompts, fragments,
questions, and pasted context. These thoughts may be used immediately or may
remain for days. They are not a FIFO queue and do not have to be sent anywhere.

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

### The board is not a queue

Visual order is spatial organization, not execution order. A thought can be
used out of sequence, revised repeatedly, or left untouched for days. The
product does not label thoughts as pending or done.

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

The product works without an account or network connection. It has no telemetry
by default. It does not inspect agent conversations or project contents merely
because it was launched from a project directory.

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

 n new   enter edit   y copy   x cut   s send   u undo   ? help
```

The green focus gutter is the strongest routine visual element. Notes have no
heading row or decorative card chrome. Whole-thought controls can appear for
the focused or hovered thought without permanently consuming a row.

The board spends no permanent row on a repeated product header. The footer
combines the optional session name, thought count, mode, and durability into one
responsive summary. Separate rows contain labeled actions and only the verified
agent targets that currently exist. Transient information and success messages
share the left side of the summary row. Warnings and errors temporarily replace
the summary so they can use the complete row. Status never changes footer
height. Narrow panes shorten or remove secondary labels before any two regions
can collide.

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

Adjacent visible thoughts use a deliberate three-row cadence when the viewport
has room: one blank row, one quiet horizontal rule, and one blank row. Shallow
panes compress that cadence to the rule alone. The rule belongs to the board
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
internal scrolling.

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
  thought. Unicode names, spaces, quotes, local file URLs, and common terminal
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
up or pressing `Esc` leaves the insertion row without creating anything.

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

### Submit to an adjacent agent

When Proqi runs beside one or more agent panes in a supported terminal
environment, it can submit the focused thought directly to a verified adjacent
agent above, below, left, or right.

This is a progressive enhancement. Copy and cut remain available in every
terminal. Submission controls appear only when an installed integration can
identify an eligible adjacent agent with confidence.

Herdr is the first supported integration. It provides directional pane lookup,
agent detection, session identity, readiness state, and an agent-aware prompt
operation. Proqi uses that semantic operation instead of simulating arbitrary
terminal keystrokes.

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

Prompt delivery has two dispositions over the same immediate semantic submit
operation. `Submit & remove` is the default and deletes the thought only after
an accepted matching receipt. `Submit & keep` sends the same prompt and retains
the thought. Proqi verifies at submission time that the target exposes Herdr's
semantic request and receipt contract. It never substitutes raw key injection.

Each verified adjacent target appears once in the integration row, without its
readiness label. `s Submit & remove` and `S Submit & keep` are shown only when
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
| Submit and remove after acceptance | `s`, when supported, then direction when needed | Click verified Submit & remove control |
| Submit and keep thought | `S`, when supported, then direction when needed | Click verified Submit & keep control |
| Undo board action | `u` | Click undo control when visible |
| Move thought | `J` and `K`, or `Shift+↑` and `Shift+↓` | Drag thought handle |
| Expand or collapse | `Space` | Click overflow indicator |
| Search | `/` | Click search control |
| Help | `?` | Click help control |
| Exit | `q` | Click exit control |

Final bindings remain configurable. The product must not depend on terminals
forwarding `Cmd+C`, `Cmd+V`, or Meta keys consistently.

### Meta and primary shortcuts

Proqi supports familiar modifier shortcuts when the terminal reports them. In
the user-facing keymap, `Meta` means the platform's primary application
modifier: Command on macOS and Control on Windows and Linux. Internally this is
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

The default theme is `auto`, with explicit `light` and `dark` overrides.
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
- `tracing` for file-based diagnostics.

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

The skill teaches an agent to discover Proqi's installed capabilities and use
its stable CLI. It does not duplicate application logic. It supports explicit
user requests to list or find sessions, inspect a specified thought, add a
thought from standard input, and perform reversible thought operations.

Agent-facing commands provide versioned JSON, opaque IDs, structured errors,
standard-input support for arbitrary text, and operation receipts. Mutations to
an active session are forwarded to its owning Proqi process so they pass through
the same reducer, lease, persistence, and undo rules as TUI actions.

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

The project is intended to become a public open source project, not merely a
personal tool that happens to be published.

Before the first public release it requires:

- A clear OSI-approved license. The recommended choice is dual MIT or
  Apache-2.0, subject to an explicit project decision.
- Public architecture and contribution documentation.
- A code of conduct and security reporting policy.
- One cross-platform development command surface shared by contributors,
  coding agents, and CI.
- Required pull-request checks for formatting, linting, documentation, tests,
  supported platforms, the minimum Rust version, and dependency policy.
- A protected default branch whose stable aggregate check must pass before
  merge.
- Reproducible release automation, checksums, an SBOM, and build-provenance
  attestations.
- Dependency license review and generated notices where required.
- Weekly Cargo and GitHub Actions updates. Automatic merging is limited to
  reviewed low-risk patch policy after all required checks pass.
- Third-party workflow actions pinned to immutable revisions with least-privilege
  permissions.
- A public roadmap that distinguishes product commitments from ideas.
- Tests on macOS, Linux, and Windows.
- No embedded personal paths, private data, or assumptions about one agent
  harness.
- Optional integrations that fail closed for submission and leave the standalone
  scratchpad fully usable.

Distribution begins with platform-specific GitHub Releases and a personal
Homebrew tap. The intended installation experience is:

```text
brew install <tap>/<formula>
```

Once the project meets Homebrew's maturity and maintenance expectations, a
submission to Homebrew Core can be considered. Shell and PowerShell installers,
and optional `cargo install` distribution, complement Homebrew.

Package managers own updates in the first public version. The application does
not silently replace its own executable.

Release archives are tested as installed products rather than only as compiled
binaries. The release gate covers installation, launch, terminal restoration,
session resumption, and compatibility with an existing database. Published
artifacts are immutable and the Homebrew package refers to their checksums.

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

## Later opportunities

These remain compatible with the vision but are not initial requirements:

- Optional session naming and deterministic aliases.
- Duplicate, merge, and split thought operations.
- External editor handoff through `$VISUAL` or `$EDITOR`.
- Import and export as plain text, Markdown, or JSON.
- Configurable retention and recoverable pruning.
- Session handoff between machines without making cloud sync mandatory.
- Additional multiplexer and harness adapters that remain separate from the
  clipboard-first core.
- A library API for third-party frontends.

## Success criteria for the first usable release

The first release succeeds when a user can keep several instances beside
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
- The interface remains visually quiet after hours of continuous use.
