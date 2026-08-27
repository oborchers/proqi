# Hermes harness qualification

This record applies the contract in
`context/HARNESS_QUALIFICATION_CHECKLIST.md` to the Herdr harness kind
`hermes`. Evidence was recorded as assertions and outcome summaries only. Raw
pane IDs, session IDs, private paths, prompts, transcripts, and credentials were
discarded.

## Qualification record

- [x] Harness kind: `hermes`.
- [x] Harness version: Hermes `0.20.6` (upstream build dated 2026-08-27,
  commit `39f1e188`).
- [x] Herdr version and protocol: Herdr `0.8.0`, protocol `19`, schema `1`.
- [x] Proqi commit: the `harness/hermes` qualification commit containing this
  record, based on `54547cd`.
- [x] Platforms and terminals: macOS 15.7.1 on arm64, in a real Herdr terminal
  workspace with the official Hermes integration installed.
- [x] Live provider/model: documented `openai-api` provider with `gpt-5-mini`;
  tool approvals remained enabled throughout qualification.
- [x] Qualification date: 2026-08-27.
- [x] Result: `pass`.
- [x] Evidence: recorded JSON fixtures in `tests/fixtures/herdr/hermes`, focused
  adapter/UI tests, the reviewed mixed-harness snapshot, the complete canonical
  gate, and the sanitized live observations below.

## Completion rule

- [x] Every required checklist area is accounted for below.
- [x] The sessionless/provisional conditional section is marked not applicable
  with a concrete reason.
- [x] Recorded tests are deterministic and require no installed harness,
  credentials, user configuration, live server, or wall-clock timing.
- [x] Live tests used the real Hermes harness inside Herdr. Private credentials
  were sourced only inside isolated test shells and were not printed or passed
  in Herdr command arguments.
- [x] No secret, transcript, database, runtime artifact, raw opaque identifier,
  or machine-specific path is retained as evidence.
- [x] `cargo xtask check` passed on the final implementation before commit.

## 1. Herdr agent contract

### Detection and identity

- [x] Canonical `herdr agent start --kind hermes` launched the official Hermes
  executable and waited for interactive readiness.
- [x] `agent list` and `agent get` agreed on pane, workspace, tab, kind, and
  user-facing name.
- [x] `agent` was stable lowercase `hermes` and matched
  `agent_session.agent`.
- [x] The established session had nonempty `kind`, `source`, and `value`; its
  value was stable throughout the conversation.
- [x] Starting a new conversation changed identity. Resuming that conversation
  by its in-memory identity preserved it, without recording the value.
- [x] Live names were unique and cleared after normal exit or replacement.
- [x] Ordinary shells, unrelated TUIs, and stale labels were not classified as
  Hermes. The regression test
  `recorded_hermes_target_survives_unrelated_adjacent_shells` also proves that
  unrelated shell neighbors do not hide a valid Hermes neighbor.
- [x] Proqi's display-only metadata was absent from Herdr coding-agent reports.

### Readiness and lifecycle

- [x] Launch-pending and explicitly noninteractive Hermes fixtures fail closed.
- [x] A live interactive settled Hermes reported `idle`.
- [x] A semantic prompt produced an observable `working` transition.
- [x] Completion settled according to Herdr's seen/unseen `idle`/`done`
  semantics.
- [x] A real harmless tool-approval surface reported `blocked`; the action was
  denied and did not execute.
- [x] `unknown` is not eligible as proof of completion in the generic policy.
- [x] Pane focus and bounded reads did not corrupt lifecycle or session state.
- [x] Normal exit cleared the live agent and session promptly.
- [x] Forced termination of a disposable Hermes foreground process group
  cleared stale identity promptly and left the shell owner intact.

### Semantic prompt operation

- [x] `agent prompt` received one argument as data, never as shell syntax.
- [x] A harmless semantic prompt exercised spaces, quotes, newlines, a tab,
  Unicode, combining characters, emoji, leading/trailing whitespace, and shell
  metacharacters. Exact construction is covered deterministically without
  retaining the private live text.
- [x] One operation yielded one `agent_prompted` receipt and one response.
- [x] The receipt matched pane, workspace, tab, kind, and established session.
- [x] A settled prompt produced an observable lifecycle transition.
- [x] Prompting while working was accepted by Hermes/Herdr as a follow-up; Proqi
  did not reinterpret that provider behavior. Protocol 19 still lacks a unique
  turn boundary for overlapping senders.
- [x] Structured failure coverage includes timeout, rejection, process failure,
  malformed output, unsupported protocol, and a live stale-target
  `agent_not_found` result.
- [x] Prompt/wait teardown is bounded by the gateway and process ownership
  tests; no orphan test process remained.

## 2. Proqi discovery and target verification

- [x] Outside `HERDR_ENV=1`, Proqi remains a scratchpad without submit targets.
- [x] Proqi negotiates protocol support and validates both request and receipt
  schemas.
- [x] Discovery validates distinct source/target panes, workspace, tab, kind,
  readiness, geometry, edge overlap, and session identity.
- [x] Directional neighbors must independently appear in both the agent list and
  layout snapshot.
- [x] Self, cross-tab, cross-workspace, non-overlapping, malformed, duplicate,
  and ambiguous candidates fail closed.
- [x] An established Hermes target appeared once with the correct direction,
  label, and display name.
- [x] Initial unsupported discovery stayed silent; explicit refresh reported
  unavailability without disturbing the board.
- [x] Deterministic focus-event coverage proves target refresh after start,
  exit, and replacement. The Herdr CLI focus action in this live driver did not
  surface an observable Crossterm focus event, so that one event source could
  not be reconfirmed live; live switching remained open and refreshed through
  the independently required resize path.
- [x] Live resize bursts refreshed geometry after settling, retaining board
  content and usable controls; deterministic UI tests cover focus, cursor,
  selection, scroll, and narrow/shallow layouts.
- [x] Every submission revalidates the complete target immediately before
  semantic delivery.

## 3. Required user stories

### Capture and navigation baseline

- [x] `n`, Enter, paste, and insertion-row mouse paths are covered by the full
  UI suite; the live insertion-row mouse target created exactly one thought.
- [x] Empty-board double-Down durable-blank behavior passes deterministic UI
  coverage.
- [x] Final-thought insertion-row and subsequent double-Down behavior passes.
- [x] Escape, Up, reorder, and unrelated actions reset insertion confirmation.
- [x] Repeated downward movement while editing one blank cannot multiply it.
- [x] Exact multiline/Unicode content remains durable across edits, board mode,
  refresh, resize, and target selection in deterministic coverage.

### One adjacent harness

- [x] One eligible Hermes exposed direct `Submit` and `Submit & keep` controls.
- [x] Live keep sent one unique harmless marker and preserved its thought.
- [x] Live remove deleted only after a matching accepted receipt.
- [x] The deletion was durable and one undo restored it.
- [x] In-flight edits prevent removal in deterministic submission coverage.
- [x] Failure, timeout, ambiguity, rejection, target change, and receipt
  mismatch all preserve source content.
- [x] Proqi displayed acceptance independently of Hermes's later answer.

### Multiple thoughts

- [x] Two selected thoughts were sent once in board order.
- [x] The exact payload uses one blank line between sources in deterministic
  payload tests; the live response confirmed one combined delivery without
  retaining the payload.
- [x] Keep preserved both live sources.
- [x] Remove deleted both accepted unchanged sources as one undoable operation.
- [x] Failed or ambiguous multi-source outcomes preserve every source.

### Multiple adjacent harnesses

- [x] Two or more eligible agents never caused destination guessing.
- [x] Both submit dispositions entered directional targeting.
- [x] Arrow keys and `h`, `j`, `k`, `l` route only to enabled directions.
- [x] Escape canceled without delivery or source mutation.
- [x] A live SGR mouse click on a Hermes indicator routed only to that target.
- [x] Narrow and shallow chooser behavior is covered by deterministic rendering
  and an explicitly reviewed snapshot.

### Mixed-harness row

- [x] Both `Claude | Proqi | Hermes` and `Hermes | Proqi | Codex` showed the
  verified kinds and correct arrows.
- [x] Submit opened the direction chooser in both layouts.
- [x] Left/`h` reached only the left harness.
- [x] Right/`l` reached only the right harness.
- [x] Each target received its unique marker once and retained independent
  lifecycle/session metadata.
- [x] Replacing both sides while Proqi remained open refreshed the controls
  without restarting Proqi.

### Four directions

- [x] Herdr reported four distinct edge-overlapping neighbors.
- [x] Proqi rendered up, right, down, and left targets.
- [x] Submit exposed all four directional choices.
- [x] Keep delivered a unique marker upward only.
- [x] Keep delivered a unique marker rightward only.
- [x] Keep delivered a unique marker downward only.
- [x] Keep delivered a unique marker leftward only.
- [x] Every direction returned an accepted matching receipt and retained the
  source thought.

## 4. Session creation and empty-harness behavior

- [x] Fresh official-integration startup produced a stable established identity
  before Hermes became eligible; no manual conversation bootstrap was needed.
- [x] During the brief launch interval before the integration hook reported the
  session, Proqi hid Hermes rather than weakening identity checks.
- [x] Hermes appeared automatically after its valid identity was reported and
  discovery refreshed.
- [x] The generic open established-session path handles `hermes`; no
  harness-specific provider branch or closed kind enum was added.
- [x] Product logic does not branch on Hermes, so a `*_AGENT_KIND` production
  constant is not applicable. Wire-format literals remain confined to recorded
  fixtures/tests.

### Conditional provisional-session policy: not applicable

The official Hermes integration reports an established session before Proqi
eligibility on a fresh startup. Therefore Hermes does not need or qualify for a
provisional-session exception. The conditional documentation, replacement
guarantee, first-receipt-before-hook handling, rediscovery, and provisional
first/follow-up tests are not applicable. The established first receipt and an
established follow-up were both verified; a sessionless, launch-pending Hermes
and a receipt that drops or changes an established session fail closed. This
avoids the protocol-19 same-pane/same-kind provisional replacement race rather
than accepting it.

## 5. Switching, races, and failure safety

- [x] An established other harness was replaced by Hermes in the same pane
  while Proqi remained open; kind/session controls refreshed.
- [x] Hermes was replaced by another harness in the same pane while Proqi
  remained open; the old target disappeared and the new one appeared.
- [x] Replacement observed before submission produced `target changed` and sent
  nothing.
- [x] A replacement after revalidation but before Herdr acceptance remains the
  documented protocol-19 integration race: receipt mismatch preserves source,
  but there is no atomic expected-instance precondition.
- [x] A changed session in an otherwise matching Hermes receipt is rejected.
- [x] Wrong submission, pane, tab, workspace, direction, and kind receipts are
  rejected by generic regression coverage.
- [x] Post-delivery readiness, display-name, and geometry changes do not
  invalidate an otherwise matching receipt.
- [x] Recovery converts prepared-but-unsent work to `cancelled` without retry.
- [x] Recovery converts sent-without-outcome work to `outcome_unknown` without
  retry.
- [x] Concurrent submissions retain the documented protocol-19 turn-boundary
  limitation.

## 6. Privacy, durability, and process quality

- [x] Integration metadata and journal exclude prompt content; only ordered
  source identifiers and redacted digests are durable.
- [x] Journal fingerprints exclude raw pane and session identifiers.
- [x] SQLite transactions are closed before invoking Herdr or Hermes.
- [x] One active attempt locks each source against incompatible mutation.
- [x] Prompt delivery uses argument vectors/standard input, never a
  shell-interpolated prompt.
- [x] Errors, logs, snapshots, fixtures, and this record contain no credential
  or private conversation content.
- [x] Terminal/process setup, cancellation, ownership, and cleanup remain
  panic-safe and bounded.
- [x] Copy/cut remain available when integration or Hermes is unavailable.
- [x] Proqi has no raw-terminal-key delivery fallback.

## 7. Automated evidence

- [x] Hermes recorded fixtures cover idle/working identity, launch pending,
  exit, replacement, and an accepted prompt receipt.
- [x] Discovery coverage includes valid/invalid identity, readiness, workspace,
  tab, geometry, edge overlap, ambiguity, shell neighbors, and four directions.
- [x] Submission coverage includes exact payload, receipt matching/mismatch,
  timeout, rejection, malformed output, and target replacement.
- [x] Variable session timing tests are not applicable because Hermes is never
  provisionally eligible; established receipt loss/change is tested.
- [x] UI coverage includes one/multiple/mixed targets, keyboard/mouse routing,
  keep/remove, failure, and in-flight source mutation.
- [x] Deterministic narrow, wide, tall, shallow, and resize tests pass.
- [x] The mixed Claude/Hermes snapshot diff was reviewed explicitly; no
  `.snap.new` file remains.
- [x] Focused Hermes adapter and UI tests passed during development and again
  before commit.
- [x] `cargo xtask check` passed immediately before commit.
- [x] `cargo xtask audit` and `cargo xtask package` are not applicable: this is
  a harness qualification commit, not a release milestone.

## 8. Live Herdr smoke evidence

- [x] `HERDR_ENV=1` was verified before control commands.
- [x] Smoke work used one isolated test tab and only test-created panes. Opaque
  IDs were held in memory for control and intentionally omitted here.
- [x] Real Hermes was started through `herdr agent start`, never simulated with
  display metadata.
- [x] Sanitized `agent get` assertions were captured before first prompt, while
  working, after completion, and after exit.
- [x] Unique harmless response markers were verified only in recent terminal
  buffers as needed to establish receipt/output, then discarded.
- [x] Proqi's accepted status was verified separately from later answers.
- [x] One-target, mixed-row, four-direction, switching, fresh-start, first
  established receipt, and established follow-up stories all passed.
- [x] Agents exited, test thoughts were removed, isolated state was deleted, the
  test tab was closed, and original focus/layout was restored.
- [x] Final `git status --short` contained only the intended implementation,
  test fixtures, snapshot, and this record before commit.
- [x] Unsupported scenario: Hermes provisional/sessionless delivery is not
  supported because the official integration supplies established identity.
  Known protocol-19 replacement and concurrent-turn limits are documented
  above and are not weakened for Hermes.
