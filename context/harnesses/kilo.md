# Kilo harness qualification

Status: passed for established Kilo sessions

This is the unique qualification record for Herdr harness kind `kilo`. It
applies to the implementation commit containing this file, based on
`54547cd`. It records only redacted contract evidence; it contains no pane or
session identifiers, private paths, credentials, or conversation transcripts.

## Qualification record

- [x] Harness kind: `kilo`
- [x] Harness version: Kilo CLI 7.5.5
- [x] Herdr version and protocol: 0.8.0 / 19
- [x] Herdr integration: official Kilo state plugin v4
- [x] Proqi commit: this qualification commit on `harness/kilo`, based on
  `54547cd`
- [x] Platform and terminal: macOS 15.7.1 arm64, Herdr-managed terminal panes
- [x] Qualification date: 2026-08-27
- [x] Result: pass for established sessions; a fresh sessionless Kilo remains
  deliberately ineligible until Kilo creates or resumes a conversation
- [x] Deterministic evidence:
  `src/adapters/herdr/tests/kilo.rs`, `tests/ui_board/kilo.rs`, and the redacted
  JSON under `tests/fixtures/herdr/kilo`
- [x] Live evidence: isolated Herdr tabs using the branch Proqi binary, the
  real Kilo executable, `gpt-5-mini`, and the documented in-memory provider
  configuration with approvals enabled

## Completion rule

- [x] Every required item below is checked.
- [x] The conditional sessionless section is marked not applicable to Proqi
  delivery, with the remaining product limitation stated explicitly.
- [x] Deterministic tests use recorded redacted JSON and do not depend on live
  credentials, user configuration, timing, or a Herdr server.
- [x] Live tests used the real Kilo harness inside Herdr; credentials were
  loaded only in private test shells outside the repository.
- [x] No secret, transcript, local database, runtime file, raw terminal/session
  identifier, or machine-specific path is committed.
- [x] `cargo xtask check` passes after the final implementation.

## 1. Herdr agent contract

### Detection and identity

- [x] `herdr agent start` with kind `kilo` started the canonical executable and
  waited for interactive readiness.
- [x] `agent list` and `agent get` agreed on pane, workspace, tab, kind, and
  unique user-facing name in the isolated tabs.
- [x] The stable lowercase `agent` value was `kilo` both at the agent and
  `agent_session.agent` boundaries.
- [x] Established sessions reported nonempty `kind=id`, `source=herdr:kilo`,
  and stable values.
- [x] Replacement/new conversations produced new values; an explicit `-s`
  resume reported the original value after the session hook ran.
- [x] Live names stayed unique and cleared when their Kilo occupants exited or
  were replaced.
- [x] Shells, the branch Proqi TUI, and stale display labels were not detected
  as Kilo agents.
- [x] Proqi's leased `proqi` display metadata never became an agent identity and
  cleared on normal exit.

### Readiness and lifecycle

- [x] Sessionless, launch-pending, and explicitly noninteractive Kilo fixtures
  are hidden.
- [x] Live `idle` corresponded to an interactive Kilo ready for input.
- [x] A live semantic prompt produced an observed `working` transition.
- [x] Completed unseen work settled as `done`; seen work settled as `idle` or
  `done` according to Herdr focus semantics.
- [x] A harmless shell-tool request produced a real permission surface and
  Herdr reported `blocked`; the request was rejected without bypassing
  approval.
- [x] `unknown` remains ineligible and is never treated as completion by the
  generic adapter tests.
- [x] Focusing and reading Kilo panes did not corrupt lifecycle or session
  tracking.
- [x] Normal exit promptly removed the live name and session report.
- [x] Forced termination of one isolated Kilo process group cleared its live
  identity within bounded polling.

### Semantic prompt operation

- [x] `herdr agent prompt` accepted prompt text as data.
- [x] The Kilo regression passes leading/trailing whitespace, quotes, newlines,
  a tab, Unicode, a combining mark, emoji, and shell metacharacters as one exact
  argument with no shell interpolation.
- [x] Each live operation produced one Kilo submission and one
  `agent_prompted` receipt.
- [x] Accepted receipts matched pane, workspace, tab, kind, and established
  session; replacement-session receipts fail closed.
- [x] Prompts from settled state produced observed working/done transitions.
- [x] A prompt sent while Kilo was working was accepted as follow-up input and
  processed after the active turn; Proqi does not reinterpret that policy.
- [x] Timeout, process failure, malformed output, rejection, receipt mismatch,
  and unsupported protocol remain structured, non-destructive failures in the
  generic Herdr adapter and UI tests.
- [x] Prompt and wait commands used explicit bounded timeouts; cleanup left no
  child harness or blocked wait.

## 2. Proqi discovery and target verification

- [x] Outside `HERDR_ENV=1`, the adapter executes no Herdr command and exposes
  no direct target.
- [x] Proqi negotiates schema 1/protocol 19 and verifies both `agent.prompt` and
  `agent_prompted` shapes.
- [x] Discovery validates distinct source/target panes, workspace, tab, open
  harness kind, readiness, geometry, edge overlap, and required session
  identity.
- [x] Every directional neighbor is independently checked against the agent
  list and layout snapshot.
- [x] Self, cross-tab, cross-workspace, non-overlapping, invalid-geometry,
  duplicate, and ambiguous candidates fail closed in deterministic tests.
- [x] One established Kilo appeared exactly once with the correct arrow,
  `Kilo` label, and live display name.
- [x] Passive unsupported discovery remained silent; explicit refresh reported
  that no verified adjacent agent existed.
- [x] Refresh after live start, exit, and replacement updated the same running
  Proqi process.
- [x] The existing resize/debounce and UI suites cover geometry refresh without
  losing thought focus, cursor, selection, or scroll bounds.
- [x] Every submission revalidated the complete Kilo target before invoking the
  semantic prompt.

## 3. Required user stories

### Capture and navigation baseline

- [x] Existing board tests cover `n`, Enter on the insertion row, board paste,
  and the insertion-row mouse target.
- [x] Empty-board double-Down creates one durable blank and enters edit mode.
- [x] Final-thought navigation focuses the insertion row, and the subsequent
  double-Down creates one blank.
- [x] Escape, Up, unrelated commands, and reorder reset insertion confirmation.
- [x] Repeated Down in the same empty edited blank creates no extra thought.
- [x] Multiline Unicode survives edit, board mode, resize, refresh, and exact
  submission tests.

### One adjacent Kilo

- [x] One established live Kilo exposed direct `s Submit` and
  `S Submit & keep` actions.
- [x] Live Submit & keep returned acceptance and preserved the source.
- [x] Live Submit removed the unchanged source only after acceptance.
- [x] Undo restored the successfully submitted-and-removed thought.
- [x] In-flight mutation locking is covered by the UI submission tests.
- [x] Timeout, ambiguity, rejection, replacement, and mismatch preserve all
  source content in adapter/UI tests; stale live replacement also kept the
  thought.
- [x] Proqi displayed accepted status before and independently of Kilo's later
  response.

### Multiple thoughts

- [x] Selected sources submit once in board order with one blank line between
  exact contents.
- [x] Submit & keep retains every selected source.
- [x] Submit removes all unchanged selected sources as one undoable operation
  only after accepted journaling.
- [x] Failed and ambiguous outcomes preserve every selected source.

### Multiple adjacent harnesses

- [x] Two or more targets always entered directional targeting.
- [x] `s` and `S` preserved remove/keep disposition in the chooser.
- [x] Arrow and `h/j/k/l` routing is covered deterministically; live `h`, `j`,
  `k`, and `l` reached only the selected side.
- [x] Escape cancellation is covered by the generic UI interaction suite.
- [x] Mouse target selection routes through the same verified direction.
- [x] Narrow and shallow deterministic layouts remain operable; the live
  55-by-23 center pane showed all four Kilo arrows and accepted keyboard
  direction choices.

### Mixed-harness rows

- [x] Live `Claude | Proqi | Kilo` showed both kinds with correct left/right
  arrows and never preferred a harness automatically.
- [x] Live `Kilo | Proqi | Codex` showed both kinds with correct left/right
  arrows and never preferred a harness automatically.
- [x] Left/`h` and right/`l` reached only the selected harness in both layouts.
- [x] Each selected harness received its unique marker once and exposed
  independent lifecycle/session metadata.
- [x] Replacing Claude with Kilo and Kilo with Codex while Proqi stayed open
  refreshed the controls without restarting Proqi.
- [x] The Kilo-specific UI regression covers the candidate in both mixed-row
  positions.

### Four directions

- [x] Herdr reported distinct edge-overlapping Kilo neighbors above, right,
  below, and left of one live Proqi pane.
- [x] Proqi rendered `↑`, `→`, `↓`, and `←` Kilo targets.
- [x] Submit entered a four-way chooser.
- [x] Submit & keep sent one unique marker upward only.
- [x] Submit & keep sent one unique marker rightward only.
- [x] Submit & keep sent one unique marker downward only.
- [x] Submit & keep sent one unique marker leftward only.
- [x] Every direction returned an accepted Proqi status, retained the thought,
  and produced exactly one Kilo reply marker in the selected buffer.

## 4. Session creation and empty-harness behavior

### Required default

- [x] A fresh Kilo is interactive before it has a conversation identity; Proqi
  did not consider it eligible.
- [x] Live explicit refresh beside that fresh Kilo showed no verified target.
- [x] After a native Kilo prompt caused the official hook to publish a stable
  identity, refresh exposed the Kilo target automatically.
- [x] Established Kilo uses the generic open `HarnessKind` and required-session
  policy with no Kilo-specific product branch.
- [x] No `KILO_AGENT_KIND` constant was added because product logic does not
  branch on Kilo.

### Conditional sessionless delivery: not applicable

Kilo 7.5.5 does not publish a stable session until its first prompt begins,
even when the official Herdr Kilo integration is installed. Proqi does not
deliver that first prompt: unlike Codex, Kilo has no reviewed provisional
identity exception. The user must begin or resume the Kilo conversation
natively, after which established-session support is complete.

- [x] The missing pre-prompt identity and user-visible limitation are
  documented above.
- [x] The official Herdr session hook is installed and was preferred over any
  weakened Proqi identity policy.
- [x] Provisional Kilo support is not proposed; no replacement guarantee is
  substituted for a missing session ID.
- [x] Sessionless revalidation, provisional receipt timing, first-prompt
  disposition, immediate post-receipt rediscovery, later provisional-to-
  established delivery, and the protocol-19 same-kind replacement race are
  not applicable because Proqi sends no sessionless Kilo prompt.
- [x] Established Kilo targets still revalidate pane, workspace, tab,
  direction, geometry, kind, and exact session immediately before delivery.
- [x] Established receipts that lose or change session identity fail closed.

## 5. Switching, races, and failure safety

- [x] An established Claude was replaced by Kilo in the same pane while Proqi
  remained open; refresh showed the new kind and session.
- [x] Kilo was replaced by Codex in the same pane while Proqi remained open;
  refresh removed the old Kilo target and showed Codex.
- [x] Submitting against a target replaced before revalidation failed as target
  changed and sent no semantic prompt in the Kilo regression; the live stale
  target attempt also preserved its source.
- [x] Protocol 19's post-revalidation/pre-acceptance replacement race remains
  documented in the product contract; mismatched receipts preserve sources.
- [x] A different Kilo session in the receipt is rejected even when pane and
  kind match.
- [x] Submission ID, pane, tab, workspace, direction, and harness mismatches
  are rejected by the submission/UI identity tests.
- [x] Post-delivery readiness, display-name, and geometry changes do not
  invalidate a matching accepted receipt.
- [x] Journal recovery maps `prepared` to `cancelled` without retry.
- [x] Journal recovery maps `sending` to `outcome_unknown` without retry.
- [x] Protocol 19's concurrent-sender turn-boundary limitation remains
  documented; Kilo accepts working-state input as follow-up.

## 6. Privacy, durability, and process quality

- [x] Submission metadata stores ordered source IDs and digests, never prompt
  content.
- [x] Journal fingerprints exclude raw pane and agent-session identifiers.
- [x] Herdr invocation occurs after the journal's short transaction closes.
- [x] One active attempt locks every source against invalid mutation.
- [x] Herdr uses argument vectors and never shell-interpolates prompt content.
- [x] Fixtures and errors contain no credential or conversation content.
- [x] Terminal setup, process ownership, cancellation, and teardown remain the
  existing panic-safe bounded paths.
- [x] Copy and cut remain available when Kilo is sessionless or unavailable.
- [x] Proqi has no raw-key delivery fallback.

## 7. Automated evidence

- [x] Recorded redacted Kilo JSON covers established detection/identity,
  sessionless and unready states, exit, accepted receipt, replacement before
  delivery, and replaced-session receipt.
- [x] Generic discovery tests cover valid/invalid identity, readiness,
  workspace, tab, geometry, edge overlap, ambiguity, and all directions.
- [x] Generic plus Kilo submission tests cover exact payload, acceptance,
  mismatch, timeout, rejection/error mapping, malformed output, and target
  replacement.
- [x] Kilo provisional receipt-timing tests are not applicable because
  sessionless Kilo is never eligible.
- [x] UI tests cover one/multiple targets, Kilo in both mixed positions,
  keyboard/mouse direction choice, keep/remove, failure, and in-flight locks.
- [x] Existing deterministic UI suites cover narrow, wide, tall, shallow, and
  resize behavior.
- [x] No production-visible label changed; representative snapshots remain
  valid and no `.snap.new` file exists.
- [x] Focused Herdr adapter and UI tests pass.
- [x] The final canonical gate passes immediately before commit.
- [x] `cargo xtask audit` and `cargo xtask package` are not applicable: this is
  a harness qualification milestone, not a release milestone.

## 8. Live Herdr smoke evidence

- [x] `HERDR_ENV=1` was verified before control commands.
- [x] All live work used isolated tabs and only panes created for this
  qualification.
- [x] Every Kilo was started through `herdr agent start --kind kilo`.
- [x] `agent get` evidence was checked before a first prompt, while working,
  after completion, while blocked, after normal exit, and after forced exit.
- [x] Unique exact markers were verified only in the selected Kilo detection
  buffers.
- [x] Proqi acceptance was verified independently before Kilo's response.
- [x] One-target, both mixed rows, four directions, switching, stale-target
  safety, working follow-up, and fresh-session hiding were exercised.
- [x] Tool approval remained enabled; the one requested permission was rejected.
- [x] Test thoughts were removed, Proqi and Kilo exited, isolated tabs were
  closed, temporary state was deleted, and the original tab/layout focus was
  restored.
- [x] Final `git status --short` contains only intended source evidence before
  commit and is empty after commit.
- [x] Unsupported scenario: Proqi cannot submit the first prompt to a fresh
  sessionless Kilo. It fails closed until the official hook reports an
  established identity; no other scenario in this checklist is unsupported.
