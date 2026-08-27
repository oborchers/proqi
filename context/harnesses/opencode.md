# OpenCode Herdr harness qualification

This record applies the repository's
`context/HARNESS_QUALIFICATION_CHECKLIST.md` acceptance contract to OpenCode.
It records conditional practical support, not full qualification. Evidence was
captured with harmless unique markers; no credential, prompt transcript, raw
pane ID, raw session ID, private path, or runtime artifact is retained.

## Qualification record

- [x] Harness kind: `opencode`
- [x] Harness version: OpenCode 1.18.23 with the official Herdr OpenCode
  integration v9 installed
- [x] Herdr version and protocol: 0.8.0 / 19
- [x] Proqi implementation commit: `19a35a7`
- [x] Platform and terminal: macOS 15.7.1 arm64, Herdr embedded terminal
- [x] Qualification date: 2026-08-27
- [x] Result: `conditional`
- [x] Deterministic evidence:
  `src/adapters/herdr/tests/opencode.rs`,
  `src/adapters/herdr/tests/fixtures/opencode/`,
  `src/adapters/herdr/tests/sessionless.rs`,
  `tests/ui_board/agent_selection.rs`, and
  `tests/ui_board/agent_session.rs`
- [x] Upstream blocker: [Herdr issue #2548](https://github.com/herdrdev/herdr/issues/2548)
  describes the OpenCode integration's missing `agent_session` report under
  this version family.

## Completion status

- [ ] Every required item is checked. Stable identity on explicit resume and
  automatic identity establishment after a resumed prompt remain unverified
  because the official hook reports no session in those cases.
- [x] The conditional sessionless section is completed below, including its
  replacement guarantee and protocol 19 race.
- [x] Deterministic tests use sanitized recorded JSON and do not depend on
  installed credentials, user configuration, timing, or a live Herdr server.
- [x] Live smoke tests used the real OpenCode harness inside Herdr with private
  credentials supplied only to the isolated test shell.
- [x] No secret, transcript, local database, runtime file, machine-specific
  path, or opaque live identity is committed as evidence.
- [x] `cargo xtask check` passed after the final implementation: 487 tests
  passed, 4 configured tests were skipped, and all Clippy, rustdoc, doctest,
  architecture, snapshot, and diff gates passed.

## 1. Herdr agent contract

### Detection and identity

- [x] Canonical `herdr agent start` launched OpenCode with `gpt-5-mini` and
  waited until `idle` and `interactive_ready=true`.
- [x] `agent list` and `agent get` agreed on pane, workspace, tab, `opencode`,
  and the live agent name.
- [x] The reported agent value was stable lowercase `opencode` and matched the
  established session's agent field.
- [x] A newly established session had nonempty kind, source, and value fields;
  its value stayed stable through working, blocked, and settled states.
- [ ] Explicitly resuming the same conversation preserves and reports the same
  identity. Both canonical `--continue` and explicit-session starts resumed to
  an interactive sessionless agent, and an accepted prompt still produced no
  `agent_session` report.
- [x] Fresh established-session replacement is covered deterministically; an
  old session is never accepted for a different established target.
- [x] Agent names were unique while live and cleared after normal exit and
  forced pane close.
- [x] Ordinary shells were not identified as OpenCode. A live shell neighbor
  also exposed and motivated the generic regression proving it cannot mask a
  verified agent in another direction.
- [x] Proqi's display-only pane metadata was never reported as coding-agent
  identity.

### Readiness and lifecycle

- [x] Launch-pending and explicitly non-interactive fixtures are ineligible.
- [x] Live `idle` was interactive and ready for input.
- [x] A harmless live tool operation produced `working` with stable identity.
- [x] Completion settled and retained the same established identity.
- [x] A harmless OpenCode question surface produced `blocked`; it was canceled
  without changing tool-approval policy.
- [x] `unknown` is not eligible or interpreted as completion.
- [x] Focusing and minimally reading the pane did not corrupt lifecycle state.
- [x] Normal exit cleared the live agent and established session promptly.
- [x] Forced close cleared stale live identity promptly; deterministic lost-
  session receipts fail closed.

### Semantic prompt operation

- [x] The semantic prompt is passed as one argument, never as shell syntax.
- [x] Sanitized deterministic coverage preserves spaces, quotes, newlines,
  tabs, Unicode, combining marks, emoji, leading and trailing whitespace, and
  shell metacharacters exactly.
- [x] One operation produced one submission and one matching
  `agent_prompted` receipt; live unique-marker checks confirmed exactly-once
  receipt.
- [x] Established receipts must match pane, workspace, tab, harness, and exact
  session identity.
- [x] Live settled prompts produced observable work and accepted receipts.
- [x] The live harness retained Herdr/OpenCode's own working-state behavior;
  Proqi does not reinterpret steering, queueing, or rejection.
- [x] Existing protocol tests cover timeout, rejection, process failure,
  malformed output, and unsupported protocol as structured failures.
- [x] Process tests prove bounded timeout, cancellation, teardown, and child
  ownership.

## 2. Proqi discovery and verification

- [x] Outside `HERDR_ENV=1`, discovery exposes no target and executes no Herdr
  command.
- [x] Protocol negotiation validates protocol 19 and the semantic prompt and
  receipt schemas.
- [x] Discovery validates source and target identity, distinct panes, matching
  workspace and tab, harness kind, readiness, geometry, edge overlap, and the
  required session policy.
- [x] Directional neighbors are independently verified against the agent list
  and layout snapshot.
- [x] Self, cross-context, non-overlapping, invalid, duplicate, and ambiguous
  candidates fail closed.
- [x] Established OpenCode appears once with its direction, `Opencode` harness
  label, and Herdr display name.
- [x] Initial unsupported discovery is silent; explicit refresh reports why a
  target is unavailable.
- [x] Refresh while Proqi stayed open detected harness start, exit, and
  replacement.
- [x] Resize settling preserves board state and refreshes geometry.
- [x] Every submission revalidates the complete target immediately before the
  semantic prompt call.

## 3. Required user stories

### Scratchpad, one target, and multiple thoughts

- [x] The canonical automated gate covers keyboard, insertion-row, paste,
  double-down creation, confirmation reset, blank-thought, edit, navigation,
  mouse, resize, multiline, and Unicode durability stories.
- [x] One live OpenCode target exposed direct `Submit` and `Submit & keep`.
- [x] `Submit & keep` preserved the thought after a matching accepted receipt.
- [x] `Submit` removed only the unchanged accepted thought, and undo restored
  it durably.
- [x] Automated in-flight mutation and submission-lock tests prevent unsafe
  source removal.
- [x] Failure, timeout, ambiguity, rejection, target change, and receipt
  mismatch preserve every source and report failure.
- [x] Acceptance was recorded without waiting for or inspecting the answer.
- [x] Automated multiple-thought tests send selected sources once, in board
  order, separated by one blank line; keep preserves all, remove is one undoable
  operation, and any failure preserves all.

### Multiple targets and mixed-harness rows

- [x] Two or more targets entered directional selection without guessing.
- [x] Remove/keep disposition survived target selection.
- [x] Arrow keys and `h`, `j`, `k`, and `l` route only to enabled directions;
  Escape cancels without mutation.
- [x] Mouse indicator routing and narrow/shallow chooser behavior pass the
  canonical automated gate.
- [x] Live `Claude | Proqi | OpenCode` and `OpenCode | Proqi | Codex` rows showed
  correct left/right labels and always entered the chooser.
- [x] Live `h` and `l` markers reached only their selected harness, exactly
  once, with independent lifecycle/session metadata.
- [x] Replacing Claude with OpenCode and OpenCode with Codex refreshed controls
  while Proqi remained open.

### Four directions

- [x] A live isolated layout reported four distinct edge-overlapping OpenCode
  neighbors: up, right, down, and left.
- [x] Proqi rendered all four arrow targets and exposed four choices.
- [x] One unique keep marker was accepted in each direction, only the selected
  OpenCode instance received it, and the source remained present.
- [x] Every direction returned a matching accepted receipt.

## 4. Session creation and provisional OpenCode policy

### Default established-session policy

- [ ] OpenCode reports stable session identity before initial eligibility. At
  interactive startup the official integration is sessionless.
- [ ] Proqi hides interactive sessionless OpenCode. This candidate uses the
  explicitly reviewed provisional exception below instead.
- [x] When a valid session is reported, discovery automatically replaces the
  provisional target with the generic established-session target.
- [x] The established-session path remains open-ended and contains no OpenCode
  provider branch.
- [x] The only kind-specific production policy uses the single canonical
  `OPENCODE_AGENT_KIND` constant.

### Conditional sessionless behavior

- [x] The official OpenCode integration can be interactive before it reports a
  session; the first live prompt receipt did contain the newly established
  session.
- [x] The official integration was retained and used unchanged. Upstream issue
  #2548 prevents preferring a complete hook fix in this version combination.
- [x] The replacement guarantee is documented: immediately revalidate the same
  pane, workspace, tab, direction, geometry, and exact harness kind, then
  accept only a matching semantic receipt.
- [x] No raw key-injection fallback exists.
- [x] Deterministic fixtures separately cover receipt-with-session and receipt-
  before-session-hook timings; the former was also observed live.
- [x] A receipt preceding the hook records success and triggers immediate
  rediscovery without resending.
- [x] Deterministic and live evidence prove the first prompt is delivered once
  and thought disposition follows the accepted receipt.
- [x] Later submissions in a normally established OpenCode session use its
  exact established identity.
- [x] An established receipt that loses or changes session identity fails
  closed.
- [ ] A resumed OpenCode instance automatically establishes identity for later
  submissions. Under Herdr 0.8.0/OpenCode 1.18.x it may remain provisional
  indefinitely, including after an accepted prompt.
- [x] Protocol 19's residual same-pane, same-kind replacement race is
  documented: no stable pre-session instance ID or atomic expected-instance
  precondition exists between revalidation and acceptance.

## 5. Switching, races, and failure safety

- [x] Live established-to-OpenCode and OpenCode-to-Codex same-pane replacements
  refreshed while Proqi remained open.
- [x] A session replacement observed before submission returns target-changed
  and sends nothing.
- [x] A replacement after revalidation is treated as the documented protocol
  19 integration race; a mismatched receipt preserves the source.
- [x] Different-session, wrong submission, pane, tab, workspace, direction, and
  harness receipts are rejected.
- [x] Volatile readiness, display name, and geometry changes after delivery do
  not invalidate an otherwise matching receipt.
- [x] Recovery marks pre-send attempts canceled and post-send unknown outcomes
  `outcome_unknown`; neither is auto-retried.
- [x] Protocol 19's concurrent-sender turn-boundary limitation is documented.

## 6. Privacy, durability, and process quality

- [x] Journal tests store only ordered source IDs and redacted digests, without
  prompt content.
- [x] Raw pane and session identities are absent from the journal fingerprint.
- [x] No SQLite transaction stays open across Herdr invocation.
- [x] Active attempts lock unsafe thought mutations.
- [x] Integration commands use argument vectors and never shell-interpolate a
  prompt.
- [x] The reviewed fixtures, errors, snapshots, and this record contain no
  credential, private transcript, opaque live identity, or machine path.
- [x] Terminal restoration, cancellation, child ownership, and cleanup are
  panic-safe and bounded under the canonical gate.
- [x] Copy and cut remain available without a target.
- [x] Proqi never injects terminal keys as a delivery fallback.

## 7. Automated evidence

- [x] Sanitized OpenCode recordings cover sessionless, established idle,
  established working, unready, replacement, exit, accepted receipt, receipt
  before hook, changed session, and lost session.
- [x] Discovery tests cover identity, readiness, context, geometry, ambiguity,
  shell neighbors, and all four directions.
- [x] Submission tests cover exact payload, acceptance, mismatch, timeout,
  rejection, malformed output, and replacement.
- [x] Both provisional receipt timings are distinct tests.
- [x] UI tests cover one and multiple targets, both mixed OpenCode positions,
  keyboard and mouse targeting, keep/remove, failure, and in-flight changes.
- [x] Narrow, wide, tall, shallow, and resize coverage passed.
- [x] Seventeen representative snapshots were reviewed; no `.snap.new` file
  exists and this change required no snapshot update.
- [x] Focused adapter and OpenCode UI tests passed during development.
- [x] `cargo xtask check` passed immediately before the implementation commit.
- [x] Release-only `cargo xtask audit` and `cargo xtask package` are not
  applicable: this is neither a release nor a release milestone, and no publish
  action was authorized.

## 8. Live Herdr smoke evidence

- [x] `HERDR_ENV=1` was verified before control operations.
- [x] Tests used only an isolated tab and panes created for this smoke run;
  opaque identities were tracked transiently and are not recorded here.
- [x] Every OpenCode instance was started through canonical `herdr agent start`,
  never simulated with display metadata.
- [x] `agent get` was checked before first prompt, while working, while blocked,
  after settlement, and after exit/forced close.
- [x] Harmless unique markers were verified only as far as necessary to prove
  receipt and direction exclusivity.
- [x] Proqi acceptance was verified independently of the later answer.
- [x] One-target, mixed-row, four-direction, switching, initial-sessionless,
  established-session, and explicit-resume stories were exercised.
- [x] Test agents exited, created panes and the isolated tab were closed, test
  thoughts and private temporary state were removed, and original focus was
  restored.
- [x] Repository status contained only the intended source and documentation
  changes; no runtime evidence was present.
- [x] Unsupported scenario: stable explicit-resume identity and automatic
  post-resume establishment cannot be qualified with the official Herdr 0.8.0
  OpenCode integration and OpenCode 1.18.x. Support therefore remains
  conditional and the overall qualification goal remains blocked upstream.
