# Cline Herdr harness qualification

Status: conditional qualification record

This record applies to the `cline` harness kind. It records deterministic
contract evidence and one private live smoke run without retaining pane IDs,
agent session IDs, paths, transcripts, or credentials.

## Qualification record

- [x] Harness kind: `cline`
- [x] Harness version: `3.0.60`
- [x] Herdr version and protocol: `0.8.0 / 19`
- [x] Proqi revision: qualification change based on `54547cd` on
  `harness/cline`
- [x] Platform and terminal: macOS arm64, `xterm-256color`, Herdr native panes
- [x] Qualification date: 2026-08-27
- [ ] Result: conditional, not pass
- [x] Evidence: focused Herdr adapter and UI tests, recorded sanitized fixtures,
  canonical gate, and the live observations below

## Completion rule

- [ ] Every required item passes. Stable Herdr session identity and truthful
  settled/blocked lifecycle reporting remain unavailable.
- [x] The sessionless conditional section was evaluated completely; unmet
  guarantees are named rather than treated as supported.
- [x] Deterministic tests do not use installed credentials, user configuration,
  timing, or a live Herdr server.
- [x] Live smoke used the real Cline harness and private credentials supplied
  outside the repository.
- [x] No secret, transcript, database, runtime file, private path, raw pane ID,
  or raw session ID is committed.
- [x] `cargo xtask check` passes for the recorded implementation.

## 1. Herdr agent contract

### Detection and identity

- [x] `herdr agent start` launched the canonical `cline` executable and
  returned an interactively ready agent.
- [x] `agent list` and `agent get` agreed on pane, workspace, tab, kind, and
  assigned name.
- [ ] `agent_session.agent` cannot be compared with `agent`: Herdr never
  reported `agent_session` for live Cline.
- [ ] No live Cline state exposed nonempty session `kind`, `source`, and
  `value`, before or after accepted prompts.
- [ ] Herdr could not prove new-conversation, replacement, or resume identity.
  Cline displayed its own resume identity only on exit, outside Herdr's agent
  contract; that value was not retained.
- [x] Agent names stayed unique while live and cleared after normal exit.
- [x] Shells and exited Cline panes were not reported as Cline agents.
- [x] Proqi's display-only pane metadata never became an agent identity.

### Readiness and lifecycle

- [x] Deterministic discovery hides `launch_pending=true` and
  `interactive_ready=false` targets.
- [x] Real `agent start` initially returned `idle` and
  `interactive_ready=true`.
- [x] Cline input produced an observable transition to `working`.
- [ ] After visible completion, Herdr continued to report Cline as `working`
  instead of settling as `idle` or `done`.
- [ ] A real Cline tool-approval surface was classified as `working`, not
  `blocked`; tool approval remained enabled and the unnecessary call was
  denied manually.
- [x] Proqi and deterministic tests treat `unknown` as ineligible before
  delivery and only advisory after an accepted receipt.
- [x] Agent reads and focus changes did not corrupt the live identity record.
- [x] Normal Cline exit promptly cleared the live agent name and detection.
- [x] Stale identities are bounded by Herdr process detection and metadata TTL;
  Proqi independently revalidates immediately before every delivery.

### Semantic prompt operation

- [x] Herdr accepted prompt text as one semantic data argument. The Cline
  regression covers spaces, quotes, newlines, a tab, Unicode, emoji, leading
  and trailing whitespace, and shell metacharacters exactly.
- [x] Each live marker operation produced one accepted `agent_prompted` receipt
  and one visible Cline response marker.
- [ ] Live receipts matched pane, workspace, tab, and kind but contained no
  established session identity.
- [x] Prompting produced observable Cline input even though lifecycle detection
  remained conservatively `working`.
- [x] Prompts accepted while Herdr reported `working` appeared once as later
  Cline input; Proqi did not reinterpret queue or steering semantics.
- [x] Timeout, rejection, process failure, malformed output, unsupported
  protocol, and receipt mismatch remain structured, non-destructive adapter
  failures in the deterministic suite.
- [x] Prompt and wait processes use the repository's bounded process runner and
  teardown contract.

## 2. Proqi discovery and target verification

- [x] Outside `HERDR_ENV=1`, Proqi executes no Herdr process and exposes no
  direct-submit target.
- [x] Protocol negotiation verifies schema 1, protocol 19, `agent.prompt`, and
  `agent_prompted`.
- [x] Discovery validates distinct source and target panes, workspace, tab,
  kind, readiness, geometry, edge overlap, and the applicable session policy.
- [x] Directional results must also exist uniquely in the agent list and layout.
- [x] Self, cross-context, non-overlapping, duplicate, invalid, and ambiguous
  candidates fail closed in deterministic coverage.
- [x] A live Cline target appeared exactly once with its correct direction,
  `Cline` label, and assigned name.
- [x] Passive unsupported discovery stays silent; explicit refresh reports the
  result without disturbing the board.
- [x] Live replacement followed by refresh changed Cline controls to Codex and
  back without restarting Proqi.
- [x] Live narrow and shallow reflow preserved the focused thought and valid
  layout; deterministic UI coverage verifies compact directional controls.
- [x] Every submission revalidates the complete target immediately before the
  semantic prompt call.

## 3. Required user stories

### Capture and navigation baseline

- [x] Existing deterministic TUI coverage proves `n`, insertion-row Enter,
  board paste, and insertion-row mouse creation.
- [x] Empty-board and final-thought double-Down creation is deterministic and
  durable.
- [x] Escape, Up, unrelated commands, and reorder input reset insertion
  confirmation.
- [x] Repeated movement in one empty blank does not create extra thoughts.
- [x] Exact multiline and Unicode content survives edit, board mode, resize,
  discovery refresh, and submission preparation.

### One adjacent harness

- [x] One live Cline target exposed direct `Submit` and `Submit & keep`.
- [x] Live `Submit & keep` produced an accepted receipt and preserved its
  source.
- [x] Live `Submit` removed its unchanged source only after acceptance.
- [x] Live undo restored that accepted removal.
- [x] In-flight source mutation is locked in deterministic UI coverage.
- [x] Failure, timeout, ambiguity, rejection, target change, and receipt
  mismatch preserve sources.
- [x] Proqi showed acceptance before and independently of Cline's later answer.

### Multiple thoughts

- [x] Deterministic tests submit selected thoughts once in board order with one
  blank line between exact contents.
- [x] Keep preserves every selected source.
- [x] Accepted remove deletes all unchanged sources as one undoable operation.
- [x] Failed and ambiguous outcomes preserve all sources.

### Multiple adjacent harnesses

- [x] Proqi entered directional targeting rather than guessing among live
  targets.
- [x] `s` and `S` retained remove and keep disposition through targeting.
- [x] Arrow keys and `h`, `j`, `k`, and `l` route only to enabled directions.
- [x] Escape cancellation and mouse target selection are deterministic.
- [x] Narrow and shallow chooser geometry is deterministic and keyboard
  operable.

### Mixed-harness rows

- [x] Live `Cline | Proqi | Codex` showed both kinds and routed independent
  markers left and right.
- [x] Live `Codex | Proqi | Cline` showed both kinds and routed independent
  markers left and right.
- [x] Direction choice never preferred a kind, readiness, or discovery order.
- [x] Every mixed-row marker appeared once only in its selected harness.
- [x] Candidate-to-Codex and Codex-to-candidate same-pane replacements refreshed
  while Proqi remained open.
- [x] Deterministic UI coverage repeats both candidate positions with `h` and
  `l` routing.

### Four directions

- [x] Live Herdr geometry exposed distinct edge-overlapping up, right, down,
  and left neighbors.
- [x] Proqi rendered all four arrow targets and entered a four-way chooser.
- [x] `Submit & keep` returned matching accepted receipts upward, rightward,
  downward, and leftward.
- [x] Each unique marker appeared once only in its selected Cline pane.
- [x] Every source thought remained present.

## 4. Session creation and empty-harness behavior

### Default established-session path

- [ ] Not applicable to the current live Cline integration: Herdr 0.8.0 has no
  installable Cline hook and never reports a Cline session before eligibility.
- [x] Cline still uses the generic established-session path whenever Herdr
  supplies a valid session. No established-session provider branch was added.
- [x] `CLINE_AGENT_KIND` is the sole canonical typed constant for Cline-specific
  provisional policy; literal `cline` values remain only in wire fixtures and
  the qualification record.

### Conditional sessionless-first-prompt path

- [x] Live evidence proves Cline cannot report a stable Herdr session before
  the first prompt.
- [x] The official installer was attempted first; Herdr 0.8.0 reports `cline`
  as an unsupported integration target.
- [x] The replacement guarantee is full pre-submit revalidation of pane,
  workspace, tab, direction, geometry, kind, and semantic receipt identity.
- [x] Proqi accepts only a matching semantic receipt and never injects raw keys.
- [x] Deterministic tests cover both receipt-with-session and
  receipt-before-session-hook timing.
- [x] A still-sessionless receipt records success and triggers immediate
  rediscovery without resending.
- [x] The first live prompt arrived exactly once and keep/remove disposition
  followed only the accepted receipt.
- [ ] Later live submissions did not use established identity because Herdr
  never reported one. They remained provisional and were fully revalidated.
- [x] Once established, a receipt that loses or changes session identity fails
  closed in Cline-specific regression coverage.
- [x] Protocol 19's same-pane, same-kind replacement race is documented: no
  stable pre-session instance ID or atomic expected-instance precondition exists.

## 5. Switching, races, and failure safety

- [x] Live established Codex-to-Cline and Cline-to-Codex switching refreshed
  controls with Proqi still open.
- [x] A live replacement before submission failed as `target changed`, kept the
  source, and delivered no marker to the replacement.
- [x] A replacement after revalidation but before acceptance remains the
  documented protocol 19 integration race; a mismatched receipt preserves the
  source but cannot prove the replacement received no text.
- [x] Different established sessions and lost sessions are rejected.
- [x] Submission, pane, tab, workspace, direction, and kind mismatches are
  rejected by typed request and receipt checks.
- [x] Volatile readiness, name, and geometry changes after delivery do not
  invalidate a matching accepted receipt.
- [x] Recovery changes `prepared` to `cancelled` and `sending` to
  `outcome_unknown`; neither state is auto-retried.
- [x] Protocol 19's overlapping-sender turn-boundary limitation remains
  documented.

## 6. Privacy, durability, and process quality

- [x] Submission metadata and journal rows contain only ordered source IDs and
  redacted digests, never prompt content.
- [x] Journal target fingerprints exclude raw pane and harness session IDs.
- [x] Herdr runs only after the preparation transaction closes.
- [x] One active attempt locks every source against unsafe mutation.
- [x] Herdr uses argument vectors; prompts are never shell interpolated.
- [x] Errors, diagnostics, snapshots, and fixtures contain no credentials or
  private conversation content.
- [x] Terminal setup, process groups, cancellation, and teardown remain bounded
  and panic-safe.
- [x] Copy and cut remain available without Herdr or Cline.
- [x] Proqi has no raw-key delivery fallback.

## 7. Automated evidence

- [x] Sanitized Cline fixtures cover sessionless and established detection,
  readiness, prompt receipts, replacement, and exit.
- [x] Discovery coverage includes identity validity, readiness, context,
  geometry, edge overlap, ambiguity, and four directions.
- [x] Submission coverage includes exact payload, accepted and mismatched
  receipts, timeout, rejection, malformed output, and target replacement.
- [x] Both first-receipt session timings are tested separately and exactly once.
- [x] UI coverage includes single and multiple targets, both mixed Cline rows,
  keyboard and mouse direction choice, keep, remove, failure, and in-flight
  mutation.
- [x] Narrow, wide, tall, shallow, and resize behavior remains deterministic.
- [x] The generic Cline label introduces no new representative snapshot; no
  `.snap.new` file exists.
- [x] Focused adapter and UI tests pass.
- [x] `cargo xtask check` passes immediately before commit.
- [x] Audit and package are not applicable: this is a harness qualification,
  not a milestone or release gate.

## 8. Live Herdr smoke evidence

- [x] `HERDR_ENV=1` was verified before control commands.
- [x] All smoke work used one isolated test tab and only panes created there.
- [x] Every real Cline process started through `herdr agent start`.
- [x] Structured identity was inspected before input, while working, after
  visible completion, and after exit.
- [x] Unique markers were verified in only the intended harness buffers.
- [x] Proqi acceptance was verified independently before later answers.
- [x] One-target, both mixed rows, four directions, switching, provisional
  first prompt, and pre-submit replacement were exercised.
- [x] Agents exited normally, temporary panes and state were removed, credential
  variables were unset, the isolated tab was closed, and the original layout
  remained.
- [x] Repository status contains no runtime evidence.
- [x] Unsupported live scenarios are named below.

## Unsupported live scenarios

1. Herdr 0.8.0 / protocol 19 exposes no Cline `agent_session`, including after
   accepted prompts. Later submissions therefore cannot transition to an
   established identity.
2. The official Herdr integration installer has no Cline target.
3. Herdr's current Cline detector reports a visible tool approval and a visibly
   completed answer as `working`, not `blocked` and `idle` or `done`.
4. Protocol 19 cannot atomically prevent same-pane, same-kind replacement after
   Proqi's final revalidation and before prompt acceptance.

These limitations prevent a full `pass` result. Proqi's conditional support is
fail-closed everywhere the current Herdr contract provides an observable
identity boundary; it does not claim guarantees the provider cannot supply.
