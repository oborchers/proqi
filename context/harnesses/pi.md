# Pi harness qualification

This is the unique qualification record for Herdr harness kind `pi`. It records
sanitized outcomes only: no pane identifier, harness session value, private
path, prompt transcript, credential, runtime database, or raw log is retained.

## Qualification record

- [x] Harness kind: `pi`
- [x] Harness version: `0.84.3`
- [x] Official Herdr Pi integration: version 8, installed through
  `herdr integration install pi`
- [x] Herdr version and protocol: `0.8.0 / 19` (client and server compatible)
- [x] Proqi commit: the `harness/pi` qualification commit based on `54547cd`;
  the final immutable SHA is recorded by Git and in the qualification handoff
- [x] Platforms and terminals: macOS 15.7.1 arm64, Herdr PTY,
  `xterm-256color`
- [x] Qualification date: 2026-08-27
- [x] Provider/model: IU Unified Endpoint, `gpt-5-mini`, OpenAI Responses API
- [x] Result: pass
- [x] Evidence: recorded fixtures under
  `src/adapters/herdr/fixtures/pi`, adapter regressions in
  `src/adapters/herdr/tests/pi.rs`, mixed-row UI regressions in
  `tests/ui_board/agent_pi.rs`, the canonical deterministic suite, and the
  sanitized live outcomes below

## Completion rule

- [x] Every required checklist item is accounted for below.
- [x] Conditional items are completed or marked not applicable with a concrete
  reason.
- [x] Deterministic tests use injected process responses and need no Herdr
  server, credentials, user configuration, or timing.
- [x] Live smoke tests used canonical real Pi processes inside an isolated
  Herdr tab with credentials loaded only in private test shells.
- [x] No secret, transcript, raw identifier, private path, runtime file, or
  local database is committed.
- [x] The final `cargo xtask check` passed after implementation and record
  review.

## 1. Herdr agent contract

### Detection and identity

- [x] Canonical `herdr agent start NAME --kind pi --pane PANE -- --provider
  iu-unified --model gpt-5-mini` started Pi and waited for an interactive
  post-setup state.
- [x] `agent list` and `agent get` agreed on pane, workspace, tab, kind, and
  user-facing name without retaining their raw values here.
- [x] Both `agent` fields were the stable lowercase string `pi`.
- [x] Every qualified fresh Pi reported a nonempty `kind=path`,
  `source=herdr:pi`, and stable session value before its first prompt.
- [x] A new Pi conversation and a same-pane replacement produced new session
  values. `pi --continue` preserved the prior conversation's value.
- [x] Valid names remained unique, duplicate live names were rejected, and
  names cleared after normal and forced exit.
- [x] Adjacent shells and the pre-interactive Pi trust surface were never
  returned as eligible coding-agent targets. The ordinary-shell regression
  discovered live is covered by
  `ordinary_neighbor_without_agent_identity_does_not_hide_a_valid_target`.
- [x] Proqi's display-only metadata never appeared in `agent list` as an agent
  identity.

### Readiness and lifecycle

- [x] Pi's first-use project-trust surface had no session identity and was
  therefore hidden. After ordinary trust completion, the official hook
  reported a stable session before the target became eligible. No provisional
  Pi exception was added.
- [x] Hook-reported `idle` was prompt-ready after session establishment.
- [x] A bounded live turn was captured as `working`.
- [x] Completion settled as `done`; a seen session settled as `idle` under
  Herdr's normal semantics.
- [x] Blocked-state item is not applicable to the stock Pi 0.84.3 test profile:
  no approval or question tool/surface is exposed. No approval-bypass flag was
  used. Official hook v8 supports an explicit `herdr:blocked` event if another
  Pi extension supplies such a surface; the installed profile had no emitter.
- [x] `unknown` remains ineligible in Proqi and is never treated as completion;
  deterministic readiness tests cover this contract.
- [x] Focus, detection reads, and a live resize burst left lifecycle and session
  identity intact.
- [x] Normal `ctrl+d` exit promptly cleared Pi identity and session reporting.
- [x] Forced termination of an exact, test-owned Pi process promptly cleared
  stale identity; no pane or session value was reused.

### Semantic prompt operation

- [x] `herdr agent prompt` treated one prompt as data, not shell syntax.
- [x] A live semantic prompt included leading/trailing whitespace, spaces,
  quotes, two lines, a tab, German and CJK text, a combining mark, emoji, and
  shell metacharacters. Pi returned only the expected harmless marker.
  `recorded_pi_receipt_accepts_exact_submission_and_rejects_replacement` also
  asserts the complete argument byte-for-byte.
- [x] Each prompt operation produced one accepted `agent_prompted` receipt and
  one observed marker.
- [x] Receipts matched pane, workspace, tab, `pi`, and the established session.
- [x] Settled prompts produced a working transition; Herdr retains its bounded
  `agent_prompt_stalled` failure when no transition is observed.
- [x] A live prompt submitted while Pi was working was accepted as Pi's native
  follow-up behavior and produced the follow-up marker. Proqi did not reinterpret
  that behavior.
- [x] Timeout, structured rejection, process failure, malformed JSON, and
  unsupported protocol remain typed failures in existing adapter tests.
- [x] Prompt and wait processes use the existing bounded process-group teardown
  contract; no orphan remained after live cleanup.

## 2. Proqi discovery and target verification

- [x] Outside `HERDR_ENV=1`, the unmanaged gateway executes no Herdr command
  and exposes no direct-submit target.
- [x] Protocol 19 and schema 1 negotiation verifies both `agent.prompt` and
  `agent_prompted` constants.
- [x] Source/target identity, distinct panes, workspace, tab, kind, readiness,
  geometry, edge overlap, and established session are validated.
- [x] Directional candidates are independently checked against the agent list
  and layout snapshot.
- [x] Self, cross-tab, cross-workspace, non-overlapping, invalid, duplicate, and
  ambiguous identities fail closed. A unique adjacent ordinary shell is now
  ignored without hiding a different valid target.
- [x] Each established Pi appeared exactly once with correct direction, `Pi`
  label, and configured name.
- [x] Initial unsupported discovery stayed silent; command-palette refresh
  surfaced a useful unavailable reason.
- [x] Host-focus refresh behavior remains covered by `host_focus_refreshes_adjacent_agents`.
  Live same-pane switches were refreshed while Proqi stayed open.
- [x] A live resize burst reflowed the board and refreshed mixed targets without
  losing the focused durable thought or valid scroll bounds.
- [x] Every live submission revalidated immediately. Exiting the sole Pi after
  discovery produced `target changed before submission`, sent nothing, and
  preserved the thought.

## 3. Required user stories

### Capture and navigation baseline

- [x] Live `n`, exact editing, board return, durable save, resize, target
  refresh, and submission succeeded. Deterministic keyboard and mouse tests
  cover Enter, paste, and insertion-row creation.
- [x] Empty-board and populated-board double-Down creation are covered by the
  insertion-navigation regression suite.
- [x] Escape, Up, unrelated commands, and reorder reset insertion confirmation;
  repeated Down on the created blank cannot create duplicates.
- [x] Multiline Unicode content remains exact through editing, board mode,
  resize, refresh, and submission in adapter and UI tests.

### One adjacent harness

- [x] With Pi beside three ordinary shell panes, Proqi rendered direct
  `s Submit` and `S Submit & keep` controls without a chooser.
- [x] Live submit-and-keep produced a matching receipt and preserved the source.
- [x] Live submit removed the unchanged source only after the matching receipt.
- [x] The removal was saved and a live `u` restored it.
- [x] In-flight source mutation is locked by existing UI and SQLite submission
  tests.
- [x] Live target replacement preserved the source and reported failure;
  timeout, ambiguity, rejection, and receipt mismatches are deterministic
  regressions.
- [x] Proqi displayed acceptance before checking only the later harmless answer
  marker.

### Multiple thoughts

- [x] Existing selection regressions prove one submission in board order with
  one blank line between exact contents.
- [x] Keep preserves every source; accepted remove deletes unchanged sources as
  one durable undo step; all failure/ambiguity outcomes preserve every source.

### Multiple adjacent harnesses

- [x] Two and four targets always entered the direction chooser; Proqi never
  guessed based on kind, readiness, or recency.
- [x] `s` and `S` preserved remove/keep disposition in the chooser.
- [x] Arrow and `h/j/k/l` routing is deterministic; all four arrows were used
  live.
- [x] Escape cancellation and mouse direction selection are covered by UI
  regressions.
- [x] Narrow, shallow, wide, tall, and resize behavior remains covered by the
  reviewed UI suite; live resizing kept the chooser and content usable.

### Mixed-harness rows

- [x] Live `Claude | Proqi | Pi` rendered the Pi right and Claude left.
- [x] Live `Pi | Proqi | Codex` rendered Pi left and Codex right.
- [x] Submit entered a chooser in both layouts and never preferred any kind.
- [x] Left reached only the left harness and right only the right harness.
- [x] Each target produced exactly one independent marker, receipt, lifecycle,
  and established session report.
- [x] Claude-to-Pi and Pi-to-Codex replacements refreshed controls while Proqi
  remained open. Deterministic `agent_pi` tests cover Pi in both positions and
  both `h`/`l` choices.

### Four directions

- [x] One isolated cross layout exposed distinct edge-overlapping up, right,
  down, and left Pi neighbors.
- [x] Proqi rendered `↑`, `→`, `↓`, and `←` Pi targets and a four-way chooser.
- [x] Submit-and-keep delivered one unique marker in each direction and only
  that Pi returned it.
- [x] Every direction returned a matching accepted receipt and the source
  remained present.

## 4. Session creation and empty-harness behavior

- [x] After the official hook and ordinary project trust were present, every
  fresh Pi reported stable session identity before Proqi eligibility.
- [x] A Pi that had not yet loaded the hook/session was hidden by the required
  established-session policy.
- [x] The target appeared after the valid session report and explicit/live
  discovery refresh.
- [x] Pi uses the generic open `HarnessKind` established-session path. There is
  no Pi provider branch and no Pi constant because product logic does not branch
  on this kind.

### Conditional sessionless policy

- [x] Not applicable. Official Herdr Pi integration v8 exposes a stable path
  identity at `session_start`, before the first prompt. Proqi therefore does not
  extend Codex's provisional exception to Pi.
- [x] No replacement identity substitute, provisional receipt timing, delayed
  rediscovery, first-prompt provisional disposition, or protocol-19
  sessionless replacement race applies to qualified Pi.
- [x] Established Pi receipts that lose or change session identity fail closed,
  as the recorded replacement regression proves.

## 5. Switching, races, and failure safety

- [x] Live Pi-to-Codex and Codex-to-fresh-Pi same-pane switches updated kind and
  session while Proqi stayed open.
- [x] Replacement before submission failed as `target changed` and sent
  nothing.
- [x] Protocol 19's post-revalidation replacement race remains documented: a
  mismatched receipt preserves the source, but there is no atomic
  expected-instance precondition.
- [x] Different established sessions and mismatched submission, pane, tab,
  workspace, direction, or kind receipts are rejected by receipt identity tests.
- [x] Volatile readiness, name, and geometry after delivery do not invalidate a
  matching accepted receipt.
- [x] Journal recovery converts `prepared` to `cancelled` and `sending` to
  `outcome_unknown` without retry; SQLite recovery tests cover both.
- [x] Concurrent submission remains a documented protocol-19 limitation: Herdr
  cannot guarantee a distinct turn boundary for overlapping senders.

## 6. Privacy, durability, and process quality

- [x] Submission metadata stores ordered source IDs and redacted digests, never
  prompt content.
- [x] Target fingerprints omit raw pane and harness session identifiers.
- [x] No SQLite transaction is held across Herdr or Pi execution.
- [x] One active attempt locks each source thought against unsafe mutation.
- [x] Integration uses direct argument vectors; no shell-interpolated prompt or
  raw-key delivery fallback exists.
- [x] Errors, diagnostics, fixtures, snapshots, this record, and the commit
  contain no credential, conversation, raw external response, private path, or
  raw runtime identity.
- [x] Terminal setup, cancellation, process groups, cleanup, and joins remain
  panic-safe and bounded.
- [x] Copy and cut remain available with no integration or Pi target.

## 7. Automated evidence

- [x] Recorded Pi JSON covers established detection/identity, idle and working
  readiness, launch-pending/sessionless hiding, accepted prompt receipt, and
  exit.
- [x] Discovery tests cover valid/invalid identity, readiness, workspace, tab,
  geometry, overlap, ambiguity, ordinary shells, and four directions.
- [x] Submission tests cover exact payload, accepted and mismatched receipts,
  timeout, rejection, malformed output, and target replacement.
- [x] Variable provisional receipt timing is not applicable to established Pi;
  the Codex-specific timing tests remain unchanged.
- [x] UI tests cover single/multiple targets, Pi in both mixed positions,
  keyboard and mouse direction, keep/remove, failures, and in-flight locks.
- [x] Responsive and repeated-resize tests cover narrow, wide, tall, and shallow
  layouts.
- [x] No visible UI implementation changed, existing representative snapshots
  were reviewed unchanged, and no `.snap.new` file exists.
- [x] Focused Herdr adapter and Pi UI tests passed during development.
- [x] `cargo xtask check` passed immediately before each qualification commit.
- [x] `cargo xtask audit` and `cargo xtask package` are not applicable: this is
  a harness qualification milestone, not a release milestone.

## 8. Live Herdr smoke evidence

- [x] `HERDR_ENV=1` was verified before control commands.
- [x] All live work used one isolated test tab and only its created panes.
- [x] Every Pi was started canonically as real `kind=pi`; display metadata was
  never used as identity.
- [x] Sanitized `agent get` evidence was captured before first prompt, during
  working state, after completion, after resume, and after normal/forced exit.
- [x] Only harmless unique markers were inspected to confirm receipt/output.
- [x] Proqi's accepted/kept or accepted/removed status was observed before the
  later marker check.
- [x] One-target, both mixed rows, four directions, same-pane switching,
  established first-prompt/follow-up, and first-use trust/session behavior were
  exercised.
- [x] Test thoughts were removed, processes exited, the isolated tab was closed,
  temporary state was deleted, and the original tab/focus/layout was restored.
- [x] Final `git status --short` contained only intended source, fixture, test,
  and qualification-record changes.
- [x] Unsupported scenario: stock Pi 0.84.3 exposes no approval/question
  surface in this profile, so live `blocked` cannot be produced. The official
  hook's explicit event contract is documented above; no unsupported behavior
  is presented as working.
