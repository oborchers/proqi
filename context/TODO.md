# Proqi product and editor backlog

Research draft, 2026-08-28. This file contains only work not yet shipped on
`main`. It records candidates, not promises or release scope.

## How to read this document

Effort assumes one contributor familiar with Proqi and includes focused tests,
Insta review for visible changes, documentation, and `cargo xtask check`.
Milestone and release work is additional.

| Size | Expected effort |
| --- | --- |
| XS | 0.5–1.5 engineering days |
| S | 2–5 engineering days |
| M | 1–2 engineering weeks |
| L | 2–4 engineering weeks |
| XL | 4–7 engineering weeks |

The product test for every item is whether it makes raw prompt composition,
recovery, organization, or transfer faster without turning Proqi into a task
manager or a general-purpose IDE. Exact content, truthful persistence, stable
adjacent-agent targeting, keyboard/mouse parity, resize safety, and one-action
undo remain non-negotiable.

## Proposed order

- [ ] **Public documentation:** turn shipped interaction workstream handoffs
  into concise GitHub-facing feature guides, beginning with range selection.
- [ ] **Release awareness:** package concise versioned highlights with every
  release and show them once in the session that initiated a successful update.
- [ ] **Fast paging and command discovery:** reuse the editor's five-row jump
  across every picker and scrollable overlay, add Page Up and Page Down aliases,
  and group the Commands overlay with inert headings that navigation skips.
- [ ] **Deterministic TUI fixtures:** add an `xtask` scenario/session seeder for
  repeatable live walkthroughs, regression reproduction, and stress testing.
- [ ] **High-return editing:** add exact replace-all and logical-line
  duplicate/move.
- [ ] **Attachment integrity:** preserve file, image, and folded annotation
  metadata across Proqi-to-Proqi copy/cut/paste, and prevent submission when an
  annotated asset can no longer be accessed.
- [ ] **Board flow:** search-driven selection and selected-block reordering.
- [ ] **Herdr discovery:** surface recognized live agents from every workspace
  and tab in the existing invocation picker, then add an explicit global
  delivery route without changing the adjacent-submit fast path.
- [ ] **Shared Proqi sessions:** let several interactive Proqi panes attach to
  one logical board through a single authoritative owner, with private view
  state, exclusive per-thought editing, and submission bound to the initiating
  pane's verified agent.
- [ ] **Standalone agent connectivity:** finish the architecture spike and any
  behavior-neutral prerequisite, then add non-Herdr connection only through a
  verified provider endpoint or required extension. Pi is the only current
  implementation candidate; Codex, Claude Code, and Hermes general TUI support
  must wait for stronger upstream contracts.
- [ ] **Parallel context:** add a dedicated conditional Git context row beneath
  the existing thought-count/mode/persistence row.
- [ ] **Composition:** split/extract/merge thoughts.
- [ ] **Visual feedback capture:** let exactly one Proqi session turn new OS
  screenshots into thoughts without dragging files across Herdr panes.
- [ ] **Later:** occurrence-only multi-selection if the exact replace-all
  workflow proves demand.

## Dependencies and parallel delivery

```text
Editor
├─ Exact replace-all ──> Occurrence-only multi-selection
├─ Existing sentence deletion ──> Paragraph deletion
└─ Shared logical-line ranges ──> Logical-line duplicate/move/join

Clipboard and attachments
└─ Typed annotated clipboard payload ──> Lossless Proqi round trips

Fast navigation
└─ One five-step intention ──> Editor rows + picker entries + overlay rows

Board
├─ Select all search matches
├─ Selected-block reorder
└─ Composite board/editor transaction ──> Split/extract/merge thoughts

Herdr discovery and routing
└─ Live agent catalog ──> Prompt reference insertion ──> Global semantic submit

Shared Proqi sessions
└─ Existing active-owner control ──> Snapshot and ordered change stream
   ──> Read-only attach ──> Private per-view state
   ──> Exclusive per-thought editing ──> Initiating-pane submission
   ──> Bounded owner-loss recovery

Standalone agent connectivity (no Herdr)
├─ Provider spikes: Pi, Hermes, Codex, and Claude Code complete
├─ Behavior-neutral adjacent-delivery refactor (3–5 days)
├─ Freeze provider-neutral target/assurance/receipt contracts
├─ Provider-neutral connected-target identity and connection lifecycle
├─ Verified provider endpoint or required harness extension
├─ Exclusive bind + nonce/liveness handshake + attributable receipt
└─ Connection chooser/status UX ──> Existing durable delivery journal

Pi native path
└─ Required Pi extension ──> Registry/socket protocol ──> Rust adapter ──> UI

Hermes native path
└─ Upstream-supported bridge/TUI contract ──> Adapter qualification ──> UI

Codex native path
└─ Upstream global live-session control contract ──> Adapter qualification ──> UI

Claude Code native path
└─ Approved Channel/bridge + attributable receipt ──> Adapter qualification ──> UI

Release awareness
└─ Highlight manifest/validation ──> Targeted acknowledgement ──> One-time overlay

Screenshot capture
├─ Watcher/classifier ─┐
└─ Exclusive lease ────┴─> Durable receipt/thought creation ──> Status/focus UX

Git context
└─ Injected discovery ──> Conditional status row ──> Optional durable browser data

Independent
├─ Shared five-step paging
├─ GitHub Pages feature guides
├─ Commands-overlay section headings
└─ Deterministic TUI scenario/session seeder
```

The safe parallel worktree lanes are **editor**, **board**, **Git/chrome**,
**release awareness**, **screenshot infrastructure**, and
**documentation/metadata**. Provider-specific standalone-agent prototypes may
also proceed independently after all feasibility spikes are reconciled, but the
provider-neutral connection identity, lifecycle, and chooser must land as one
shared foundation rather than being reinvented by each adapter. Keep editor
tickets in one practical lane. The commands overlay and release-highlights
overlay can proceed independently at
the model layer, but their final rendering work should be serialized. Screenshot
TUI wiring and the Git status row both touch responsive chrome and should also
be serialized. Shared-session work changes the session lease, control protocol,
editor ownership, submission coordination, and most PTY lifecycle contracts. It
should run in one dedicated lane and reconcile those concurrent changes before
implementation rather than copying them from open worktrees.

## Herdr discovery and global delivery

### Submit to any live Herdr agent: P1, M (7–12 days after discovery)

User story: submit one thought or the current multi-thought selection to a
recognized coding agent in another tab or workspace on the current Herdr server
without moving panes or relying on raw terminal input.

- [ ] Add an explicit **Submit to agent...** chooser. Keep `s` and `S` as the
  adjacent-agent fast paths, and never reinterpret an inserted reference as a
  delivery target.
- [ ] Replace the direction-only target contract with a discriminated route:
  `AdjacentPane(Direction)` preserves existing topology and receipt semantics;
  `HerdrAgent` carries a verified workspace, tab, pane, harness, and session
  identity without fabricated geometry or sentinel directions.
- [ ] Migrate the content-redacted submission journal with a versioned route
  kind and optional adjacent direction. Decode every legacy attempt exactly and
  recover in-flight legacy states conservatively.
- [ ] Revalidate the exact global target immediately before `agent.prompt`.
  Permit provider-supported `idle`, `done`, and `working` delivery; show
  `blocked` and `unknown` targets but disable submission with truthful feedback.
- [ ] Reuse the existing source locks, multi-thought prompt assembly, accepted
  receipt matching, outcome-unknown recovery, and remove-only-after-acceptance
  contract. Never fall back to raw text or key injection.
- [ ] Scope the first version to the current Herdr server. Do not imply discovery
  across named remote servers, SSH hosts, or historical sessions that Herdr
  cannot currently prove are live.
- [ ] Cover target rename, movement, disappearance, session replacement,
  duplicate display names, concurrent submissions, changed source content,
  accepted and rejected receipts, restart recovery, migration, diagnostics,
  and equivalent keyboard and mouse flows.

## Product positioning

### Publish GitHub-facing feature guides — P1, S (2–4 days initially)

User story: after a substantial interaction feature ships, users can learn its
complete keyboard and mouse workflow, portability fallbacks, configuration,
and intentional limitations without reading implementation PRs or internal
engineering context.

- [ ] Build a checked-in GitHub Pages documentation site modeled on the
  [`oborchers/pydantypes`](https://github.com/oborchers/pydantypes) repository
  and its published
  [`oborchers.github.io/pydantypes`](https://oborchers.github.io/pydantypes/)
  documentation. Use those as the reference for repository structure,
  navigation, presentation quality, and publication workflow while adapting the
  content and visual language to Proqi.
- [ ] Keep the public documentation versioned with this repository and linked
  prominently from the README; do not split canonical guidance across an
  unversioned wiki and checked-in files.
- [ ] Start with a range-selection guide covering Shift+Up/Down, the terminal-
  safe `v` latch, arrows/J/K, click and Shift-click behavior, switching back to
  arbitrary Space selection, Escape/edit transitions, supported bulk actions,
  and the selected-block-reorder non-goal.
- [ ] Derive each guide from the reviewed product contract and final shipped
  behavior. A workstream handoff is evidence, not publishable copy.
- [ ] Exclude internal Herdr workspace/pane IDs, worktree paths, branch names,
  commit SHAs, CI bookkeeping, private research sources, and implementation-only
  architecture from public user documentation.
- [ ] Give suitable feature guides a short rendered GIF demonstration in the
  same polished terminal style as the primary README demo. Use one focused GIF
  to show the complete interaction sequence (including its visible result), not
  as decorative motion or as a substitute for written keyboard and mouse
  instructions.
- [ ] Add a feature-level GIF only when the behavior is both practical to
  record deterministically and materially easier to understand in motion—for
  example range construction, list indentation, multi-occurrence replacement,
  screenshot capture, or another short state transition. Prefer a reviewed
  static screenshot, terminal transcript, or no media for configuration,
  background processing, error tables, nonvisual commands, very long flows, or
  behavior whose accurate demonstration would require private state, unstable
  external services, or misleading mockups.
- [ ] Capture demonstrations from the exact shipped build with isolated,
  synthetic data. Keep paths, session identifiers, credentials, private
  prompts, unrelated panes, and machine-specific chrome out of every frame.
  Check in or document the reproducible capture recipe and source recording so
  a changed workflow can be regenerated rather than manually approximated.
- [ ] Keep animations compact, legible at the documentation content width,
  free of rapid flashing, and slow enough to follow. Provide descriptive alt
  text plus a representative static frame or concise step list so the guide
  remains complete for reduced-motion users and when animation does not load.
- [ ] Include compact terminal examples or reviewed screenshots only when they
  materially clarify behavior, and keep keyboard-only and mouse-only workflows
  equally complete.
- [ ] Link relevant settings, command-palette fallbacks, help text, compatibility
  notes, and troubleshooting from each guide without duplicating the complete
  README.
- [ ] Add a lightweight release checklist item requiring public documentation to
  be created or updated when a shipped user workflow materially changes.
- [ ] Validate links, privacy, narrow-screen readability, and agreement with the
  current product contract before publishing.

### Show concise release highlights after an in-app upgrade — P1, M (5–9 days)

User story: after I approve an in-app Proqi upgrade, the same session shows one
small, readable summary of the most important newly installed features so I can
use them immediately. Other concurrently restarted Proqi sessions remain quiet.

- [ ] Add one checked-in, versioned, machine-readable release-highlight
  manifest that is packaged with the installed product and available without a
  network request. Keep the GitHub Release note as the complete external
  changelog; the in-app manifest contains only three to six high-signal,
  user-facing highlights per version.
- [ ] Bind every highlight entry to an exact stable Proqi version. When an
  upgrade skips versions, show the packaged entries newer than the previously
  installed version through the new version, grouped by version in one
  scrollable overlay.
- [ ] After a successful Homebrew update, durably target the pending announcement
  to the exact session in which the user confirmed **Update and restart all
  sessions**. Do not infer the target from whichever participant happens to
  restart first or last.
- [ ] Show the announcement only after that initiating session has resumed under
  the confirmed new executable and restored its board. Peer sessions restarted
  by the same coordinator never show the automatic announcement.
- [ ] Reuse the responsive frame, close target, scrolling, focus treatment, and
  keyboard/mouse behavior of the current `proqi shortcuts` overlay. Title it
  `what's new in Proqi X.Y.Z`; do not use the searchable command-palette layout
  or introduce a new visual language.
- [ ] Treat the announcement as acknowledged only when the user dismisses it by
  keyboard or mouse. A crash or restart before acknowledgement shows it again;
  dismissal is durable and suppresses that exact upgrade announcement forever.
- [ ] Add an always-available **What's new** command-palette action that can
  reopen the installed highlights without changing the one-time automatic
  acknowledgement.
- [ ] A failed, cancelled, ambiguous, or partially restarted update creates no
  false announcement. If the initiating session cannot restart immediately,
  retain the pending announcement for its next successful resume at the target
  version. Missing or mismatched packaged highlights fail quietly and
  truthfully rather than showing an empty or wrong-version overlay.
- [ ] Keep this local and content-free: store only session identity, previous and
  target versions, and acknowledgement state. Do not add telemetry, fetch
  release notes after startup, or show the overlay for an external package
  replacement that no Proqi session initiated.
- [ ] Extend the release skill so every release preparation drafts and reviews
  the compact in-app highlights alongside `.github/release-notes/vX.Y.Z.md`,
  verifies exact version agreement, and includes both in the pre-commit release
  review. The skill must never invent highlights from commit titles alone.
- [ ] Enforce the same contract through `xtask` and CI so the maintainer skill is
  guidance rather than the only gate: a release version without matching,
  bounded, valid packaged highlights fails release preparation and packaging.
- [ ] Test coordinator-only display across multiple active sessions, skipped
  versions, successful dismissal, crash-before-dismissal, aborted installation,
  partial restart, delayed coordinator resume, and corrupt or missing manifest
  state. Add reviewed light/dark/limited-color snapshots plus narrow, shallow,
  scrolling, keyboard, mouse, resize, and real update-restart PTY coverage.

### Share five-step paging across editors and overlays: P1, S (2 to 4 days)

Current contract: `Alt+Up` and `Alt+Down` already move the editor cursor by
exactly five wrapped visual rows while preserving its preferred terminal-cell
column. `Page Up` and `Page Down` are currently unused. The shared interaction
should offer both spellings without changing ordinary Up/Down behavior.

User story: while editing a long thought or navigating a large picker, I can use
the same fast-navigation shortcut to advance through content in predictable
five-step increments instead of repeatedly pressing Up or Down.

- [ ] Introduce one normalized fast-previous and fast-next intention below raw
  terminal input. Map both `Alt+Up` / `Alt+Down` and `Page Up` / `Page Down` to
  it in supported modes. Do not let individual overlays reinterpret raw key
  codes independently.
- [ ] In edit mode, move exactly five wrapped visual rows, preserve the preferred
  terminal-cell column, keep the cursor visible through internal scrolling, and
  clamp at the first or last row. Shift plus either spelling extends the current
  text selection through the same movement.
- [ ] In every selectable picker, overlay, browser, and future pointer-local
  context menu, move exactly five selectable entries. Display-only headings,
  separators, disabled structural rows, and overflow cues do not count toward
  the five entries. Clamp at the first or last eligible entry instead of
  wrapping across a boundary.
- [ ] For scrollable overlays with no selected entry, including shortcuts and
  release highlights, move exactly five visible content rows and clamp at the
  scroll bounds. Keep ordinary Up/Down behavior unchanged where those keys
  already have a more granular meaning.
- [ ] Give every potentially long picker one bounded viewport and explicit
  scroll offset. Keep the selected entry visible after fast navigation,
  filtering, asynchronous result replacement, and resize. Show quiet overflow
  cues when more content exists above or below; a permanent scrollbar is not
  required in shallow terminal panes.
- [ ] Keep mouse-wheel events inside the topmost open surface. They scroll that
  picker or overlay and never the obscured board or editor. Recompute hit
  geometry from the same visible slice after every wheel, paging, filter, and
  resize event.
- [ ] Preserve board-mode contracts. Plain and shifted vertical input continue
  to focus and select thoughts, while Primary plus Shift continues to reorder.
  Do not make the new Page keys or Alt paging an accidental second board reorder
  or five-thought selection command without a separate product decision.
- [ ] Derive README controls, contextual help, shortcut overlays, remapping
  labels, and command discovery from the same semantic definition. Document the
  existing Alt spelling and the new Page-key alias together.
- [ ] Cover empty, one-entry, fewer-than-five, exact-five, and long inventories;
  first and last boundary clamping; headings; filtered and asynchronously
  replaced results; editor selection; Unicode width; narrow and shallow resize;
  mouse-wheel isolation; repeated key input; every picker and scrollable
  overlay; reviewed snapshots; and real PTY translation of both key spellings.

### Group the Commands overlay with section headings: P1, S (2 to 4 days)

User story: when the Commands overlay contains many interactions, I can scan
stable sections such as editing, board organization, submission, session tools,
and help instead of reading one undifferentiated list.

The motivating example is the empty-query Commands view in which one tall flat
list runs from **New thought** through editing, clipboard, submission, session,
recovery, board, help, and **Quit Proqi** actions. This is the searchable global
Commands overlay opened with `:`, not a pointer-local context menu; use that
terminology consistently in product text, documentation, and implementation.

- [ ] Model headings explicitly as display-only rows, separate from executable
  command entries. A heading has no command identifier, accelerator, enabled
  state, or execution path.
- [ ] Keep the selected item invariant on an executable command. Up/Down and all
  other next/previous navigation skip headings immediately in either direction;
  wrapping, paging, scrolling, and initial selection must never land on one.
- [ ] Give mouse input the same contract: headings may render and receive hover
  geometry, but clicking or moving across one never selects or executes it and
  proceeds to the next eligible command when directional navigation applies.
- [ ] Filter only executable commands. Render a section heading only when at
  least one visible matching command belongs to it; never leave orphaned,
  duplicated, or consecutive empty headings in search results.
- [ ] Keep categories stable and user-facing rather than mirroring Rust modules.
  Review the complete command inventory before naming or ordering the groups,
  and keep frequently used commands easy to reach.
- [ ] Keep the primary overlay flat and searchable. Do not introduce category
  submenus: section headings provide enough visual hierarchy without hiding
  commands or adding mode-management overhead.
- [ ] Permit at most one purpose-built second disclosure layer after command
  activation, and only when the action inherently needs another choice or
  confirmation, such as a destination session, delivery target, takeover, or
  destructive confirmation. Never nest a third layer, and never use the second
  layer merely to browse command categories.
- [ ] Render headings as quiet structural labels consistent with Proqi's current
  overlay language. Their non-interactive state must remain understandable in
  automatic, light, dark, and limited-color themes without relying on color
  alone.
- [ ] Preserve the existing keyboard scroll contract: Up/Down and key repeat
  keep the selected executable command visible through the complete inventory.
  Reuse the shared five-step paging intention for both Alt+Up / Alt+Down and
  Page Up / Page Down. Headings never count as selectable stops.
- [ ] Make mouse-wheel input over an open Commands overlay scroll its command
  rows and never the obscured board or editor underneath. Keep clicked and
  hovered hit geometry aligned with the newly visible rows after every scroll.
- [ ] Show quiet, non-interactive overflow cues when commands exist above or
  below the visible slice so a mouse-only user can discover that the list
  continues. Do not spend a permanent row on a traditional scrollbar when the
  pane is already shallow.
- [ ] Preserve responsive behavior: constrain the overlay to the viewport,
  ellipsize labels on terminal-cell and grapheme boundaries, and keep the query,
  selected command, close target, and overflow state truthful after resize. At
  the currently covered `30×5` shallow size, keyboard and mouse must reach every
  command. Below the minimum actionable height, render one explicit too-small
  state rather than invisible selectable rows or hit targets.
- [ ] In shallow layouts, selection visibility takes priority over keeping a
  section heading onscreen. When at least two result rows fit, include the
  selected command's immediately preceding heading where practical; never let
  headings consume the only actionable row.
- [ ] Cover empty and single-section results, filtering across sections,
  forward/reverse navigation, first/last wrapping, mouse hit-testing, scrolling,
  wheel isolation from underlying content, resize while scrolled, narrow/shallow
  layouts including `30×5`, the explicit too-small state, and reviewed
  representative snapshots.

## Smart text editing

### Paragraph deletion — P1, S (2–5 days)

- [ ] Add **Delete paragraph** through the command palette first, using
  blank-line-delimited blocks. Keep it distinct from both a newline-delimited
  logical line and a width-dependent visual row.
- [ ] Define separator and list-structure ownership exactly, preserve
  annotations and exact line endings, and commit one persistent undo step.
- [ ] Assign no prominent default shortcut until real sentence-deletion use
  demonstrates that the broader destructive unit remains predictable.

### Logical-line duplicate and move — P1, M (4–7 days)

- [ ] Reuse the canonical logical-line range and touched-unit machinery from
  logical-line deletion and smart-list indentation rather than introducing a
  second definition of line boundaries or selection endpoint behavior.
- [ ] In edit mode, make Primary+D duplicate the selected text or current logical
  line; retain thought duplication in board mode.
- [ ] Move touched logical lines with Alt+Up/Down plus configurable/palette
  fallbacks.
- [ ] Operate on newline-delimited logical lines, never wrapped visual rows.
- [ ] Preserve exact LF/CRLF style, missing final newline, selections, and
  annotations.
- [ ] No-op cleanly at document boundaries.
- [ ] Do not silently renumber Markdown lists.
- [ ] Add “Join selected logical lines” as a separate explicit command, with a
  documented separator and exact one-step undo semantics.

## Repeated-text editing and multi-cursor

### Exact replace-all in the current thought — P0, S (2–4 days)

This captures most of the proposed multi-cursor value with much less new state.

- [ ] Use explicit selected text, or infer the Unicode word at the primary caret.
- [ ] Show the exact match count in a compact replacement prompt.
- [ ] Replace literal, case-sensitive, non-overlapping matches as one revision and
  one persistent undo step.
- [ ] Preserve exact content outside matches and correctly rebase annotations.
- [ ] Cover composed/decomposed Unicode explicitly: visually similar text is not
  equal unless its stored bytes/code points match the chosen exactness contract.
- [ ] Cap or efficiently stream very large match sets; avoid quadratic behavior.

### Occurrence-only multi-selection — P2, L (8–14 days)

- [ ] Introduce a normalized non-empty `SelectionSet` of sorted, non-overlapping
  logical ranges with one primary selection.
- [ ] Introduce atomic `TextTransaction` changes applied against the original
  document, with a complete resulting selection set.
- [ ] Apply disjoint changes from the end of the document backward, or use an
  equivalent offset-mapping algorithm whose result cannot depend on edit order.
- [ ] Primary+D selects the word or adds the next exact occurrence in edit mode.
- [ ] Add “select all occurrences” and “skip next occurrence” shortcuts plus
  command-palette fallbacks.
- [ ] Type, Enter, Backspace, Delete, and paste operate simultaneously across all
  ranges as one transaction.
- [ ] Paint secondary selections/carets while keeping the one native terminal
  cursor at the primary range.
- [ ] Persist before/after selection sets with content revisions so persistent
  undo restores them; do not persist selection-only navigation.
- [ ] Exclude matches hidden inside collapsed folds initially, with truthful
  feedback that expansion is required.

## Board selection and organization

### Select all search matches — P0, XS (0.5–1.5 days)

- [ ] In search, offer “Select all matches” for a nonblank query.
- [ ] Replace the current board selection with the matching live thoughts in
  board order.
- [ ] Reuse existing bulk actions and preserve the query until the user exits.
- [ ] Offer “Collapse all matches” using the same ordered match set, without
  changing nonmatching thoughts.
- [ ] Report `N thoughts selected` after search-driven selection.
- [ ] Show a truthful empty result and never select deleted thoughts.

### Selected-block reorder — P2, S/M (3–6 days)

- [ ] Move a contiguous selected block up/down while preserving internal order.
- [ ] Define discontiguous behavior explicitly: reject with feedback or compact
  into a stable block before moving; do not guess silently.
- [ ] Persist the reorder atomically and restore it with one board undo.

## Thought composition

### Split, extract, and merge thoughts — P1, M (5–9 days)

- [x] Split at the logical cursor with an exact boundary: left keeps its identity,
  right becomes a new thought immediately below, and neither side is trimmed.
- [x] Extract the exact active selection into a new thought and close the gap in
  the source.
- [x] Merge contiguous selected thoughts with exactly one configured separator
  (default one blank line), keep the first identity, and recoverably delete the
  sources.
- [x] Partition, shift, or merge annotations without losing attachment/fold
  identity.
- [x] Reject locked or noncontiguous inputs with actionable feedback.
- [x] Implement one atomic composite durable operation across editor and board
  history; undo must survive restart and FTS must remain consistent.
- [ ] Offer “split by blank lines” only as an explicit previewed bulk action, not
  an automatic transformation.

### Annotation-preserving clipboard round trips — P1, S/M (3–6 days)

User story: when I copy or cut a complete image/file placeholder, an annotated
selection, or thoughts containing annotations and paste them elsewhere in
Proqi, every placeholder and fold remains intact even when the content also
contains ordinary prose.

- [ ] Preserve the current external contract: the system clipboard always
  receives the exact canonical plain text, including underlying absolute paths,
  so content remains useful in terminals, editors, and other applications.
- [ ] Add a typed internal clipboard representation carrying validated relative
  annotation ranges and kinds. Choose an OS clipboard flavor or another
  current-user mechanism that can prove it belongs to the exact current plain-
  text payload; never attach stale metadata merely because text happens to
  match.
- [ ] Project copied annotations into selection-relative ranges. For complete
  board selections, shift annotations through the canonical inter-thought
  separator without losing their order or identity.
- [ ] Keep collapsed annotations atomic. Copying or cutting a visible image,
  file, or pasted-text placeholder operates on its complete canonical range and
  cannot expose or remove only part of its hidden content.
- [ ] Make cut non-destructive until both the interoperable text and the
  annotation-preserving Proqi payload have been accepted. On failure, retain the
  source and report truthfully instead of silently degrading a requested move
  into metadata-losing plain text.
- [ ] Prefer a verified typed payload on paste, then retain the current
  best-effort existing-path reconstruction as the fallback for plain clipboard
  text and terminal file drops.
- [ ] Never move, copy, rewrite, upload, or delete a referenced source file as a
  side effect of copy, cut, or paste. Missing files retain truthful canonical
  content and must not cause unrelated annotations to be discarded.
- [ ] Reuse the same annotation slicing, shifting, validation, and rebasing
  owner for editor selections, whole-thought operations, split/extract/merge,
  duplication, and session transfer rather than creating parallel range logic.
- [ ] Cover attachment-only and mixed-prose thoughts, multiple images/files,
  large-paste folds, Unicode paths, repeated identical paths, multi-thought
  separators, copy and cut failure, external clipboard replacement, stale or
  malformed typed metadata, missing files, restart/process boundaries, and one-
  step persistent undo after a cut.

## Shared Proqi sessions

### Attach several interactive views to one authoritative board: P1, XL (5 to 9 weeks)

User story: when several coding agents are working on the same task, I can open
the same logical Proqi board beside each agent. Every pane sees durable board
changes promptly, while its focus, cursor, selection, scroll position, hover,
Compose buffer, and open overlays remain private. An action initiated from one
pane is serialized once, appears in the other panes, and retains one truthful
persistent undo history.

Product boundary: this is a shared local board, not a distributed rich-text
editor. Exactly one authoritative Proqi process retains the session lease,
reducer, storage lane, operation sequence, submission journal, and durable
history. Attached views never write around that owner or mutate the session
database directly. Do not introduce multi-writer SQLite, CRDT, operational
transformation, terminal screen scraping, or raw key forwarding.

- [ ] Start with a focused architecture spike. Freeze the shared-state and
  per-view-state boundary, per-thought editing ownership, owner-loss behavior,
  shared undo semantics, initiating-pane submission contract, mixed-version
  negotiation, and exact protocol limits before changing the session lease.
- [ ] Add an explicit attach flow such as `proqi --attach <id-or-name>`. Keep
  ordinary `proqi`, continue, and resume behavior safe by default so two panes
  never begin sharing a session merely because they opened in the same
  directory.
- [ ] Extend the existing same-user active-owner control transport rather than
  creating a second command service. Authenticate the exact owner and attached
  instance, negotiate a versioned capability set, bound every frame and queue,
  and preserve current fail-closed behavior for incompatible owners.
- [ ] Introduce one canonical initial session snapshot and an ordered change
  stream keyed by the durable session operation sequence. An attached view must
  detect gaps, discard stale generations, and request a fresh canonical
  snapshot instead of applying changes out of order or guessing missing state.
- [ ] Keep durable board content, order, session name, presentation preference,
  operation history, submission locks, and persistence state shared. Keep
  focus, range selection, editor cursor and viewport, hover, mouse capture,
  local modal state, transient status, and responsive layout private to each
  attached view.
- [ ] Keep passive and engaged Compose state private until materialization.
  Concurrent first-content creation must allocate one ordinary thought per
  accepted intention through the owner, preserve every exact input byte, and
  never let two clients claim one transient Compose generation.
- [ ] Give a thought at most one interactive editing owner at a time. Different
  thoughts may be edited concurrently after the protocol proves their distinct
  ownership. A second view attempting to edit an owned thought receives clear
  read-only feedback and may retry after explicit release, disconnect, or
  bounded crash recovery.
- [ ] Route edits through typed expected-revision or expected-digest requests
  and the owner's ordinary revision pipeline. Preserve autosave truth,
  coalescing boundaries, annotations, Unicode positions, restart-safe undo and
  redo, and idempotent retries. Never resolve an edit race with last-writer-wins
  replacement.
- [ ] Route structural actions through the existing owner reducer using unique
  request and operation identities. Create, delete, duplicate, reorder,
  collapse, multi-selection, split, extract, merge, transfer, and board history
  must each produce the same single durable result they produce from the owner
  TUI today.
- [ ] Define undo as an action against the shared durable history, not a private
  visual rewind. Editor undo addresses the exact thought and revision history;
  board undo addresses the session board history. Every attached view receives
  and renders the resulting canonical change.
- [ ] Bind direct submission to the attached pane that initiated it. The owner
  must still own source capture, durability barriers, attachment preflight,
  submission locks, journal transitions, accepted receipt handling, and
  optional removal. The initiating client must discover and revalidate its own
  adjacent agent through a typed, attributable protocol. Never silently submit
  to the authoritative owner's adjacent pane.
- [ ] Retain one screenshot-inbox authority and one attachment-health policy.
  A committed capture becomes an ordinary owner mutation and is broadcast once.
  Transient filesystem observations may remain process-local only where they
  cannot cause views to disagree about whether submission is permitted.
- [ ] Make the first production milestone reconnect safely rather than elect a
  new owner automatically. When the owner exits, attached views stop admitting
  mutations, preserve any unacknowledged local editor buffer visibly, wait for
  authoritative lease release, and offer bounded reconnect or ordinary resume.
  Add automatic owner election only after deterministic failover and pending
  operation recovery have a proven contract.
- [ ] Do not let a slow or abandoned client block the owner. Use bounded
  outbound queues, generation-aware resynchronization, heartbeat or connection
  liveness, idempotent disconnect cleanup, and the existing overall bounded
  shutdown discipline. Backpressure may force one client to resnapshot, never
  stall durable work for the session.
- [ ] Stage delivery. First ship live read-only mirrors with navigation and
  resynchronization. Then add structural actions and private view state. Next
  add exclusive per-thought editing. Add initiating-pane direct submission and
  bounded owner-loss recovery only after the earlier stages are qualified.
- [ ] Cover two to fifteen attached views, concurrent edits to different
  thoughts, rejected same-thought editing, Compose races, rapid structural
  actions, shared undo and redo, storage failure, sequence gaps, slow clients,
  disconnect and reconnect, owner crash, mixed protocols, update replacement,
  screenshots, attachments, narrow and shallow resize, mouse and keyboard
  parity, Unicode and control-heavy content, direct submissions from different
  adjacent panes, and proof that no operation is duplicated or silently lost.

Effort includes the complete production path. A read-only synchronized mirror
is approximately one to two weeks. Structural actions and per-thought editing
add roughly two to four weeks. Initiating-pane submission, failure recovery,
and complete PTY qualification add another two to four weeks. The stages may
ship independently only when each has a truthful capability boundary and does
not imply unsupported collaborative editing.

## Standalone coding-agent connectivity (without Herdr)

User story: when Proqi is running outside Herdr, connect this board to one
verified live coding-agent session, see which session owns the connection, and
submit through the same exact, journaled keep/remove workflow without terminal
key injection or pretending that a historical transcript is a live agent.

This is not a generic TTY integration. A provider qualifies only when it offers
a documented or structurally reliable stable live identity, liveness proof,
semantic prompt boundary, and attributable acceptance result. Screen scraping,
raw key injection, process/TTY guessing, blind writes to session files, or
opening the same transcript in a second independent runtime remain prohibited.

### Adjacent-delivery boundary prerequisite — P0, S (3–5 days)

Architecture-spike verdict: land one small behavior-neutral refactor before any
native connection work. The existing journal, source locks, prompt assembly,
recovery, and accepted-only deletion are reusable, but the generic-looking
target/discovery boundary and submission policy still require Herdr workspace,
tab, pane, and direction identities. Provider branches started now would either
fabricate those fields, duplicate policy, or collide across the same UI and port
modules.

- [ ] Split the current agent port by responsibility. Make the existing topology
  explicit with adjacent discovery/identity/capability types and separate
  delivery and pane-presentation contracts. Keep current public Rust imports
  available through canonical re-exports or a compatibility composite so the
  refactor changes no caller behavior. Do not add native `BoundSession` fields
  in this prerequisite.
- [ ] Move construction of exact submission requests and content-redacted
  attempts, target fingerprinting, receipt matching, and accepted/failed
  deletion planning into one application-owned submission module. Board UI
  retains selection, visible status, and effect dispatch; the reducer retains
  source locks. Remove UI-owned identity hashing and duplicated policy.
- [ ] Dependency-inject adjacent discovery, submission, and pane presentation
  into the terminal external lane. Production composition still installs only
  Herdr; do not introduce a provider registry, native lifecycle, daemon, or
  product-visible connection state in this refactor.
- [ ] Keep every outward contract byte-for-byte stable: no SQLite/schema or
  storage-protocol migration, no `IntegrationContext`, CLI/JSON, diagnostics,
  snapshot, footer/help, Herdr argv/schema/receipt, or delivery-outcome change.
  Existing direction and pre-state fields remain the legacy adjacent projection
  until the later native foundation.
- [ ] Add executable architecture fixtures establishing application ownership of
  submission policy and prohibiting adapter/UI types in generic delivery
  modules. Do not game the source limit or introduce speculative provider
  abstractions.
- [ ] Preserve all existing Herdr discovery, revalidation, provisional-session,
  receipt, journal, recovery, lock, submit-all, deletion, and visual behavior.
  The external lane must be testable with independent fake discovery and
  delivery implementations, and Herdr remains the only production adapter.
- [ ] Add pure application tests for multi-source payload/digests, durability
  sequencing, exact receipt match/mismatch, changed-source retention,
  accepted-only deletion planning, journal failure, and no-resubmit recovery.
  Add fingerprint tests proving stability across display name, readiness, and
  geometry and change across provider, pane, session, or address discriminant.
- [ ] Run the focused UI agent/submission/submit-all and lock suites, SQLite
  submission contracts, Herdr executable/adapter tests, `cargo xtask
  architecture`, and final `cargo xtask check`. Review snapshots explicitly;
  this behavior-neutral prerequisite should not change them.

### Shared connection foundation — P1, L (8–12 days after prerequisite)

- [ ] Reconcile the Codex, Claude Code, Pi, and Hermes spikes into a provider
  capability matrix. Ship no adapter merely because the harness can resume or
  fork historical state in another process.
- [ ] Generalize the current Herdr-shaped adjacent target identity into one
  provider-neutral connected-session identity while leaving Herdr geometry and
  neighbor discovery inside the Herdr adapter. A native target must carry its
  provider, durable harness session identity, live process-instance identity or
  nonce, protocol version, and verified endpoint.
- [ ] Freeze a discriminated target address instead of optional fields:
  `AdjacentPane` retains Herdr topology and session-binding policy;
  `BoundSession` carries provider-scoped durable session identity plus a fresh
  live incarnation/generation and transient route handle. Keep volatile name,
  readiness, geometry, endpoint path, PID alone, and operation-specific expected
  turn IDs out of durable identity.
- [ ] Make delivery assurance explicit and finite: attributable
  provider-accepted, truly durable/idempotent provider-queued, and terminal
  bytes queued are distinct outcomes. Terminal-byte admission must never satisfy
  semantic acceptance or accepted-only deletion.
- [ ] Add an application-owned connection lifecycle—discover, bind, disconnect,
  reconnect, and explicitly confirmed takeover—over injected provider adapters.
  Domain values validate identifiers and capabilities; Unix sockets, registries,
  process metadata, permissions, and wire framing remain adapter concerns.
- [ ] Enforce one cooperating Proqi connection per harness session with a
  current-user OS lock, but treat the harness endpoint's exclusive bind as the
  cross-client authority. Metadata is explanatory, never proof of ownership.
- [ ] Revalidate durable session identity plus live instance identity before
  every send. A resumed session with a new process nonce is a new live target;
  stale discovery, replaced endpoints, incompatible protocols, and ambiguous
  liveness fail closed.
- [ ] Reuse the existing content-redacted submission journal, stable submission
  identity, exact prompt assembly, one-request delivery, accepted-only removal,
  and `outcome_unknown` recovery. Never automatically retry after acceptance
  may have occurred.
- [ ] Keep provider capabilities typed: idle-only, steer, follow-up, queueing,
  acceptance receipt, completion events, cancellation, and takeover must not be
  inferred from one another or advertised when the provider cannot prove them.
- [ ] Bound registry scans, messages, prompt sizes, deadlines, reconnects, and
  teardown. Require user-only endpoints and peer validation; never expose prompt
  content or credentials in registry metadata, locks, logs, diagnostics, or UI.
- [ ] Stress multiple live and historical sessions, two Proqi clients, another
  non-Proqi client, busy/idle delivery, concurrent and repeated prompts, exact
  multiline Unicode, provider crash, Proqi crash, restart/resume, stale
  registry cleanup, endpoint replacement, lost receipts, and duplicate risk.

### Standalone connection UX — P1, M (1–2 weeks after shared foundation)

- [ ] Outside `HERDR_ENV`, make connection discovery visible on an empty board
  with a restrained `Connect to Agent` insertion-row-style action beneath
  `+ New thought`. Finalize whether it remains visible as an installation/help
  entry when no qualified endpoint exists after reconciling all provider spikes;
  never imply that an unqualified historical session is connectable.
- [ ] Open a keyboard- and mouse-complete chooser containing only verified live,
  compatible endpoints. Show harness, bounded session name, shortened stable
  identifier, project/cwd context when safe, live status, and plugin/protocol
  compatibility without exposing private prompt history.
- [ ] Show the connected harness/session identity and truthful states such as
  connecting, idle, working, pending delivery, disconnected, incompatible, and
  outcome unknown. A socket close, session switch, or nonce change invalidates
  the connection immediately.
- [ ] Offer disconnect and explicit takeover. Never silently steal a binding,
  redirect a prompt after a target changes, or auto-connect based only on a
  recently used session ID.
- [ ] Persist only the safe durable binding preference needed to offer reconnect;
  require a fresh live handshake after Proqi or the harness restarts. Do not
  persist bearer tokens, socket trust, volatile process identity, or liveness.
- [ ] Preserve standalone prompt composition when no adapter is installed,
  compatible, or running. Native connectivity remains optional and must not
  make Herdr a runtime dependency of the scratchpad.
- [ ] Keep the first native binding transient. A later safe preference may store
  provider, harness, durable session identity, and user-facing ranking context,
  but never a socket path, token, PID/nonce/generation as reusable trust, live
  lease, or takeover state. Require a fresh handshake after every restart.
- [ ] When native targets enter durable attempts, migrate with a discriminated
  address kind and fingerprint version rather than nullable/sentinel directions.
  Preserve legacy Herdr decoding; recover any in-flight legacy attempts
  conservatively before reinterpretation; evolve diagnostics and machine JSON
  deliberately with fixtures and prepared release notes.

### Do not build a Proqi terminal multiplexer — explicit non-goal

The architecture spike also evaluated reproducing Herdr's terminal mediation.
It is technically possible only when Proqi or another documented substrate owns
the terminal from process launch. An arbitrary existing process in an ordinary
macOS/Linux terminal cannot be safely adopted later: another process cannot
recover the PTY master input channel, and PID/TTY inspection, debugger/kernel
injection, Accessibility keystrokes, or emulator internals are neither portable
nor an acceptable identity/submission contract.

- [ ] Do not embed or ship a persistent Proqi PTY broker/launcher. A production
  broker would need master-FD ownership, process groups, screen/mode tracking,
  visible attach/detach, scrollback, multi-client resize/input arbitration,
  crash supervision, and signal/job-control correctness—effectively a terminal
  multiplexer, estimated at 10–16 engineering weeks plus permanent maintenance.
- [ ] Continue using Herdr for the shipped terminal-mediated path. Its receipt
  proves bytes queued to the resolved terminal, not a distinct semantic prompt
  boundary or model acceptance; preserve its documented legacy behavior but do
  not let that weaker assurance define future native provider contracts.
- [ ] If strong demand appears later, consider only a separately scoped tmux
  adapter for sessions already running under tmux. Label its action `Send to
  terminal`, make it keep-only, and report `Terminal input queued; prompt
  boundary not verified`. Never expose remove-after-success, accepted delivery,
  or automatic retry for terminal-byte admission.
- [ ] Provider hooks may report stable session identity to an owned substrate,
  but identity reporting alone does not create an inbound semantic channel. A
  provider plugin/server remains required for exact acceptance, dedupe, and safe
  removal semantics.

### Pi extension-backed adapter — P1, L (roughly 2–3 weeks)

Spike verdict: Pi 0.84.3 is feasible only with a required Proqi-maintained Pi
extension loaded before the interactive session starts. Stock Pi exposes stable
historical session UUIDs and safe RPC only for the process that owns stdin and
stdout; it has no active registry, per-session lock, attach endpoint, live
process identity, or safe way to steer an already-running TUI. Opening the same
session file in a second RPC process creates a split-brain sibling runtime and
is not a connection. The experimental Pi server/protocol packages do not attach
to an existing TUI and are not a production contract.

- [ ] Build the smallest documented Pi extension needed for the integration;
  ship no stock/session-file/process-table/TTY fallback. Qualify every supported
  Pi release because the extension API is the compatibility boundary.
- [ ] On `session_start`, publish a user-only local endpoint and bounded registry
  record containing the durable Pi session UUID, a fresh random live-instance
  nonce, PID/start identity, safe display context, socket path, and protocol
  version. Stop admission and remove it across switch, fork, reload, shutdown,
  and crash-safe stale reconciliation.
- [ ] Require a handshake and every request to match both session UUID and live
  nonce. The extension owns one exclusive client binding; a second bind rejects,
  disconnect releases it, and takeover is an explicit confirmed protocol action.
- [ ] Serialize one delivery in flight and carry Proqi's submission identity,
  exact bounded text, intended idle/steer/follow-up mode, UUID, and nonce. Use
  Pi's documented `sendUserMessage`, never UI or terminal injection.
- [ ] Do not acknowledge the current void `sendUserMessage` call as accepted.
  Correlate the exact matching `message_start` event while briefly gating
  competing interactive input; timeout or endpoint loss becomes
  `outcome_unknown`. Initially restrict to idle-only submission if steer and
  follow-up cannot be attributed without ambiguity.
- [ ] Ask upstream Pi for a correlated acceptance/preflight return or supported
  live session service. Revisit the event-gating workaround if that lands; do
  not adopt the explicitly experimental server/client packages prematurely.
- [ ] Show only verified extension endpoints as live Pi choices. Historical Pi
  JSONL sessions may be labeled non-connectable for explanation, but their
  existence or modification time never proves liveness.
- [ ] Prototype the extension/socket contract first (estimated 2–4 days), then
  the Rust adapter and journal integration (4–7 days), followed by chooser,
  install guidance, race coverage, and cross-version qualification (4–7 days).

Pi research evidence: installed `@earendil-works/pi-coding-agent` 0.84.3 and
upstream tag `v0.84.3` were tested using isolated state and the supplied API
route. Exact Unicode/multiline RPC, busy rejection, steer/follow-up queuing,
concurrent senders, restart/resume, forced crash, and live-TUI-plus-second-RPC
were exercised. The full temporary harness state was removed after the spike.

### Hermes upstream bridge prerequisite — P2, blocked upstream

Spike verdict: Hermes Agent 0.20.6 has an official semantic JSON-RPC/WebSocket
boundary for sessions deliberately hosted by `hermes serve` or Dashboard, but
the ordinary `hermes --tui` runtime is process-private. It exposes no supported
external discovery or injection path. A serve-only preview would exclude the
normal user workflow and currently depends on internal discovery/auth details,
so it must not be presented as generic `Connect to Agent` support.

- [ ] Wait for Hermes to expose supported modern-TUI enumeration, lifecycle,
  live process identity, semantic injection, and attributable durable receipt,
  or an official bundled plugin that owns this bridge. Do not build against its
  private TUI session map, spawn ledger, token bootstrap internals, process tree,
  SQLite state, stdin, or terminal surface.
- [ ] Propose the narrow upstream bridge contract: a user-only Unix socket or
  Windows named pipe with peer-user validation; atomic live registry; durable
  session key plus process-incarnation/live identity; empty-draft lifecycle;
  TUI injection through the same busy policy as `prompt.submit`; bounded
  teardown; and durable idempotent request IDs with explicit
  accepted/queued/ambiguous/rejected receipts.
- [ ] Require one persistent verified connection and fingerprint the Hermes
  install identity, process start identity, gateway replay epoch, live session
  ID, and durable session key. A restart or socket loss invalidates the binding;
  reconnecting the durable session is explicit because it creates a new live
  identity.
- [ ] Preserve Proqi thoughts for ambiguous delivery. Hermes JSON-RPC IDs are
  correlation identifiers, not idempotency keys; replay can duplicate durable
  prompts, in-memory queues can be lost, and interrupted-turn auto-continue may
  intentionally replay work after restart.
- [ ] Do not claim exclusive writing merely because Proqi holds a local lock.
  Hermes UI and other WebSocket clients can still submit. The upstream bridge
  must provide authoritative binding or serialization semantics, or the UI must
  state the concurrency limitation truthfully.
- [ ] Require exact-boundary qualification. The tested gateway preserved
  interior multiline Unicode and shell-looking text in SQLite, but public
  history normalized outer whitespace. Queued concurrent requests were merged
  into one durable user turn, so current request-to-turn attribution is not
  strong enough for Proqi's remove-after-acceptance contract.
- [ ] If upstream lands the bridge, estimate 2–3 engineering weeks for Proqi's
  connected-target identity, adapter, lock, chooser/status UI, snapshots, and
  crash/concurrency qualification. Upstream bridge work itself was estimated at
  1–2 engineering weeks. A 3–5 day serve-only prototype remains research-only
  and must not ship as general Hermes compatibility.

Hermes research evidence: an isolated `hermes serve` process supported active
session enumeration, activation, correlated streaming/queued responses,
structured closed-target errors, restart detection, and durable-key resume.
Adversarial tests also proved duplicate replay, queued-turn merging, stale
internal ledger state, port-zero discovery failure, and the lack of any stock
TUI attach endpoint. All disposable sessions, panes, ports, and temporary state
were cleaned after the spike.

### Codex global live-session control prerequisite — P2, blocked upstream

Spike verdict: stock Codex CLI 0.150.1 cannot safely enumerate or attach to
arbitrary currently active ordinary TUI sessions. Each ordinary TUI normally
owns an embedded app-server; a second app-server sees its durable thread only as
historical/not loaded and has no semantic route into that live process. Rollout
files, SQLite, writer locks, process tables, and session indexes are undocumented
internals and still do not provide a supported submission channel.

- [ ] Wait for Codex to expose a supported global active-session registry and
  control endpoint with live process/server generation, stable thread identity,
  capabilities, and idempotent semantic submission, or for ordinary TUIs to
  adopt a universal shared app-server. Do not inspect or write rollout files,
  SQLite, session indexes, writer locks, PIDs, TTYs, or terminal input.
- [ ] Distinguish durable thread identity from liveness. `thread/list` is
  historical, and `thread/loaded/list` describes residency in one app-server,
  not whether a human-visible TUI remains attached; threads may stay loaded
  after clients disconnect.
- [ ] Preserve explicit delivery semantics. `turn/start` can race from idle into
  an implicit steer without saying which occurred. Busy steering is safe only
  with the observed active turn ID and `turn/steer`'s `expectedTurnId`.
  Experimental `thread/queue/add` is a queued-follow-up capability, not direct
  turn acceptance or completion.
- [ ] Never retry an ambiguous send automatically. Concurrent identical queue
  submissions receive different queue IDs and both execute; JSON-RPC and queue
  identifiers correlate requests but do not deduplicate them. Queue acceptance
  means durably queued, not agent completion, and later hook rejection may still
  consume the entry.
- [ ] A Proqi current-user lease may exclude cooperating Proqi connections for
  one endpoint/Codex-home/thread tuple, but cannot exclude Codex UIs, SDKs, or
  other clients. Takeover can only acquire a demonstrably released Proqi lease;
  it cannot force or truthfully claim global ownership.
- [ ] Revalidate Codex home/install identity, app-server generation/version,
  exact thread UUID, loaded state, direct-input capability, and active turn ID
  immediately before delivery. Server restart, endpoint replacement, and
  start-versus-steer races fail closed or become `outcome_unknown`.
- [ ] Treat a deliberately configured shared Codex app-server as a separate,
  opt-in experimental product profile only if explicitly approved. It may list
  compatible loaded threads over documented Unix-socket JSON-RPC and offer
  explicit idle-new-turn or expected-turn steer; it must call them “loaded Codex
  threads,” never attached or active TUIs. Historical resume is a separate user
  action.
- [ ] Do not make the experimental queue API or current shared-daemon lifecycle
  a stable Proqi dependency. A controlled Unix-socket proof of concept is
  estimated at 4–6 days; production capability negotiation, notification state,
  journal/lease integration, chooser UI, and restart/concurrency qualification
  would take roughly 2–4 engineering weeks with moderate-to-high upgrade risk.

Codex research evidence: official app-server JSON-RPC supported stable thread
UUIDs, loaded-thread enumeration within one server, exact multiline Unicode,
attributable `turn/start`, and expected-turn steering. Live experiments also
proved that a separate server cannot see an embedded TUI as loaded, loaded state
outlives TUI attachment, two identical queue submissions both execute, writer
conflicts prevent second-runtime resume, and restart preserves history but not
loaded identity. All disposable panes and isolated Codex state were removed.

### Claude Code acknowledged Channel prerequisite — P2, blocked upstream

Spike verdict: Claude Code 2.1.251 can publicly enumerate live sessions through
`claude agents --json` and exposes stable session UUIDs, but it has no supported
local command, API, SDK, or socket that submits into an arbitrary already-running
interactive session. `--resume`, `--continue`, headless mode, and the SDK create
or control a second process; concurrent resume demonstrably interleaves and
duplicates transcript branches rather than steering the original owner.

- [ ] Wait for Anthropic to make Channels available for the supported production
  authentication route and allow an official or organization-approved Proqi
  Channel plugin. Channels currently require pre-launch opt-in/allowlisting,
  remain research preview, cannot retrofit an already-open ordinary session,
  and were unavailable through the supplied authenticated gateway route.
- [ ] Require an acknowledgment and idempotency contract before shipping.
  Current Channel notification completion proves only a transport write; an
  unloaded or blocked event may be silently dropped. Proqi acceptance must wait
  for a matching deterministic plugin receipt or correlated processed/rejected
  callback, never the initial notification enqueue.
- [ ] Do not use transcript JSONL, daemon roster/job JSON, control sockets/keys,
  process stdin, binary internals, PIDs/TTYs, hooks, or screen state as an inbound
  control channel. Hooks observe/modify lifecycle context, ordinary MCP is pull
  oriented, and Remote Control is a claude.ai/mobile service rather than a local
  API supported by API keys or custom base URLs.
- [ ] Treat public session discovery separately from connectability. The chooser
  may eventually explain live `claude agents --json` rows, but enable Connect or
  Submit only after an atomically verified plugin endpoint exists. Historical
  transcripts are never live targets.
- [ ] A future plugin must publish a user-only endpoint/registry with full Claude
  UUID, owning PID/start identity, endpoint generation, and peer-user validation;
  validate all three before delivery. It must carry Proqi submission identity
  and payload digest, perform bounded dedupe, and report processed/rejected via
  a correlated callback. Transport loss or callback timeout remains
  `outcome_unknown` with no automatic retry.
- [ ] A current-user Proqi lease keyed by provider and full Claude UUID excludes
  only cooperating Proqi clients. It cannot exclude the Claude UI, Remote
  Control, Channels, SDKs, or another resume process; UI wording must say
  “connected by Proqi,” never exclusive control of Claude.
- [ ] If Channels mature and the plugin is approved, estimate 5–8 days each for
  plugin/registration/receipt/dedupe, Rust catalog/adapter/lease/journal work,
  and chooser/status/failure/concurrency/PTY qualification—roughly 3–5
  engineering weeks total. Reject the 2–4 day discovery-only adapter because it
  would expose live rows without a safe delivery capability.

Claude research evidence: isolated live discovery, stable identity across
resume, exact stored multiline Unicode, busy and idle concurrent resumes, two
simultaneous senders, duplicate retries, normal exit, crash cleanup, and
background-session lifecycle were exercised. Both idle and busy resume created
independent second-process turns; the original TUI received no semantic prompt.
The official Channel plugin was installed in isolated state but the installed
Claude build rejected Channel activation for the available auth route. All
disposable panes, sessions, clones, and isolated state were removed.

## Screenshot capture

### Exclusive screenshot inbox — P1, M/L (2–3 weeks)

User story: while reviewing visual work in another application, enable capture
on one Proqi board, take ordinary OS screenshots, and receive one new thought
per completed screenshot without focusing or dragging into the Proqi pane.

#### Linux behavior

- [ ] Use an explicitly configurable screenshot directory on Linux. Suggest the
  documented GNOME `~/Pictures/Screenshots` location when it exists, while
  remaining compatible with configurable tools such as KDE Spectacle.
- [ ] Use the same stable-file and bounded-image pipeline on inotify-backed
  systems. Do not assume that the XDG Screenshot portal reports screenshots
  initiated by other applications; it only supports capture requests.

## Parallel-agent Git context

### Git branch/worktree context — P1, S/M (2–4 days ephemeral; +3–5 durable)

- [ ] When Git context is detected, add one quiet, dedicated status row directly
  below `N thoughts · mode · persistence`. Do not squeeze Git context into the
  session-name or summary rows.
- [ ] Show repository name, branch or detached short SHA, and worktree identity.
  Give the branch the flexible width so long branch names retain as much useful
  context as possible.
- [ ] Make every field independently configurable with
  `show_git_repository`, `show_git_branch`, and `show_git_worktree`. Default all
  three to `true`; disabling one removes its segment and separators cleanly.
- [ ] Omit the entire Git row when every field is disabled or every enabled field
  is unavailable. A detached short SHA follows the branch toggle because it is
  the branch-position fallback.
- [ ] Preserve this representative expanded layout in snapshots:

  ```text
  Mouse selection QA
  2 thoughts · edit · saved
  proqi · feature/multi-cursor-foundation · worktree-name
  n New  y Copy ...
  ← Codex  s Submit ...
  ```

- [ ] Omit the entire row outside Git or when context cannot be determined. In
  shallow panes, preserve content and essential delivery state before the
  optional Git row.
- [ ] Do not collect remotes, diffs, user/email configuration, or commit logs.
- [ ] Use an injected, bounded process capability rather than shelling out from UI
  or domain code.
- [ ] Refresh on startup/resume and debounced host focus; fail silently when no Git
  context exists.
- [ ] Also refresh on an explicit user command for deterministic debugging.
- [ ] Decide separately whether this context is merely current UI state or stored
  for session browser/search. Do not persist it accidentally.
- [ ] If made durable, display and search repository/branch/worktree context in
  the session browser without exposing remotes, Git identity, or history.

## Developer tooling and live qualification

### Deterministic TUI scenario/session seeder — P0, S (2–5 days)

User story: while implementing, reviewing, or walking through a TUI feature, a
developer can create the same rich disposable Proqi session with one command,
resume it in a dedicated pane, and remove it afterward without touching any
real session or hand-entering fragile test content.

- [ ] Add a first-party `xtask` command that seeds sessions only through public
  application or CLI contracts. Do not write SQLite rows directly or depend on
  private storage layout, user configuration, ambient session state, or the
  current wall clock.
- [ ] Create a fresh isolated state root for every invocation. Refuse any target
  that could resolve to the user's normal Proqi state, and never enumerate,
  mutate, resume, trash, or prune an existing user session.
- [ ] Provide named, composable fixtures rather than one monolithic demo. At
  minimum cover short and long thoughts, several thoughts in board order,
  automatic/collapsed/expanded presentation combinations, folds and supported
  annotations, plus a combined editor stress scenario.
- [ ] Make logical newlines explicit and distinguish them from visually wrapped
  rows. Include separate exact-boundary fixtures for no trailing newline and a
  trailing newline so live evidence cannot accidentally treat them as
  equivalent.
- [ ] Include deterministic text containing ordinary ASCII, repeated tokens,
  indentation and tabs, wide CJK characters, combining sequences, emoji/ZWJ
  sequences, controls represented through the supported public contract, long
  unbroken content, and content that wraps at common narrow pane widths.
- [ ] Print the exact isolated state path, seeded session identifier, copyable
  `proqi -r ...` resume command with the required environment/configuration, the
  fixture names applied, and one exact cleanup command or path. Output must not
  include credentials or unrelated machine state.
- [ ] Make cleanup explicit, bounded, and idempotent. Normal cleanup and failed
  seeding remove only the owned disposable state; retaining a scenario for a
  walkthrough must be an intentional option with an unmistakable cleanup hint.
- [ ] Support deterministic composition of fixtures and stable seed data so a
  regression report can name the exact scenario instead of attaching private
  database files. Validate unknown, duplicate, or incompatible fixture names
  before creating durable state.
- [ ] Use the seeder in future live Herdr stress tests and walkthroughs where it
  fits, while keeping the command itself independent of Herdr. Update the
  `implement-in-worktree` and walkthrough guidance after the command exists so
  agents prefer the canonical scenario over ad hoc shell or database setup.
- [ ] Test isolation, partial-failure cleanup, repeated runs, Unicode and exact
  byte preservation, presentation combinations, terminal-width-independent
  logical content, printed resume instructions, and proof that a populated real
  state root remains unchanged.

## Explicit non-goals for this backlog

- [ ] Do not add task status, tags, due dates, agent queues, or dashboard chrome.
- [ ] Do not add AI rewrite/ranking of thoughts or silently transform prompt text.
- [ ] Do not build a general plugin marketplace or Markdown renderer.
- [ ] Do not weaken target identity checks for any submission workflow,
  including submit-all and first-prompt delivery.

## Research references

- [CommonMark list items](https://spec.commonmark.org/0.31.2/#list-items)
- [VS Code basic editing and multiple selections](https://code.visualstudio.com/docs/editing/codebasics)
- [VS Code default keybindings](https://code.visualstudio.com/docs/reference/default-keybindings)
- [Helix multiple selections](https://docs.helix-editor.com/usage.html#multiple-selections)
- [Apple contiguous and discontiguous selection](https://support.apple.com/en-ie/guide/mac-help/mchlp1378/mac)
- [Obsidian Note Composer](https://obsidian.md/changelog/2021-06-21-desktop-v0.12.6/)
- [Apple Screenshot save behavior](https://support.apple.com/en-asia/guide/mac-help/mh26782/mac)
- [Pydantypes source documentation reference](https://github.com/oborchers/pydantypes)
- [Pydantypes published GitHub Pages reference](https://oborchers.github.io/pydantypes/)
- [Apple FSEvents](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html)
- [Apple Desktop file permissions](https://support.apple.com/en-gb/guide/mac-help/-mchld5a35146/mac)
- [Linux inotify](https://man7.org/linux/man-pages/man7/inotify.7.html)
- [GNOME screenshot location](https://help.gnome.org/gnome-help/screen-shot-record.html)
- [KDE Spectacle](https://docs.kde.org/stable_kf6/en/spectacle/spectacle/using.html)
- [Rust `notify` filesystem watcher](https://docs.rs/notify/latest/notify/)
