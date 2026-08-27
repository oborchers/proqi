# New Herdr Harness Qualification Checklist

Use this checklist before Proqi treats a new coding-agent harness as a supported
Herdr submission target. It covers the deterministic contract and the live user
stories that must work inside Herdr. A harness is not qualified by detection
alone.

The candidate harness kind is written as `<kind>` below. Record evidence rather
than checking an item from memory.

## Qualification record

- [ ] Harness kind: `<kind>`
- [ ] Harness version: `<version>`
- [ ] Herdr version and protocol: `<version> / <protocol>`
- [ ] Proqi commit: `<commit>`
- [ ] Platforms and terminals: `<matrix>`
- [ ] Qualification date: `<date>`
- [ ] Result: `pass / conditional / fail`
- [ ] Evidence links or notes: `<tests, logs, screenshots, issue>`

## Completion rule

- [ ] Every required item is checked.
- [ ] Every conditional section is either completed or marked not applicable
  with a written reason.
- [ ] Deterministic tests pass without depending on installed credentials,
  user configuration, timing, or a live Herdr server.
- [ ] Live smoke tests use the real harness inside Herdr with private test
  credentials supplied outside the repository.
- [ ] No secret, transcript, local database, runtime file, or machine-specific
  path is committed as evidence.
- [ ] `cargo xtask check` passes after the final implementation.

## 1. Herdr agent contract

### Detection and identity

- [ ] `herdr agent start <name> --kind <kind> --pane <pane>` starts the canonical
  executable in an available shell pane and waits for interactive readiness.
- [ ] `herdr agent list` and `herdr agent get` identify the same pane, workspace,
  tab, harness kind, and user-facing name.
- [ ] The reported `agent` value is stable, lowercase, and matches the value in
  `agent_session.agent`.
- [ ] An established session reports nonempty `kind`, `source`, and `value`
  fields. The session value remains stable for the lifetime of that harness
  session.
- [ ] Starting a different conversation or replacing the harness produces a new
  session value. Explicitly resuming the same conversation preserves its
  identity, and an old value is never silently reused for different content.
- [ ] Agent names follow Herdr's naming contract, remain unique while live, and
  clear when the pane occupant exits or is replaced.
- [ ] A shell, ordinary TUI, or stale display label is not misidentified as the
  candidate harness.
- [ ] Proqi's display-only `proqi` pane metadata is never reported as a coding
  agent identity.

### Readiness and lifecycle

- [ ] Launch-pending or explicitly non-interactive states are not eligible for
  prompt delivery.
- [ ] `idle` means the harness is interactive and ready for input.
- [ ] Starting work produces an observable transition to `working`.
- [ ] Completion settles as `idle` or `done` according to Herdr's seen/unseen
  semantics.
- [ ] A real approval or question surface is reported as `blocked`.
- [ ] `unknown` is treated as uncertain state, never as proof of completion.
- [ ] Focusing and reading the pane do not corrupt lifecycle tracking.
- [ ] Normal exit clears the live agent and session report promptly.
- [ ] Crash, forced termination, and hook loss expire or clear stale identity
  within Herdr's documented bound.

### Semantic prompt operation

- [ ] `herdr agent prompt` accepts one prompt as data, not shell syntax.
- [ ] Spaces, quotes, newlines, tabs, Unicode, combining marks, emoji, leading
  whitespace, trailing whitespace, and shell metacharacters arrive exactly.
- [ ] One prompt operation produces one harness submission and one
  `agent_prompted` receipt.
- [ ] The receipt identifies the same pane, workspace, tab, harness kind, and
  established session as the verified request.
- [ ] A prompt sent from a settled state produces an observable lifecycle
  change or Herdr returns a bounded `agent_prompt_stalled` failure.
- [ ] Prompting while the harness is working has documented behavior: steer,
  queue/follow-up, or reject. Proqi does not reinterpret this choice.
- [ ] Timeout, rejection, process failure, malformed output, and unsupported
  protocol are returned as structured failures.
- [ ] Prompt and wait operations have bounded teardown and do not leave an
  orphan child process or unbounded join.

## 2. Proqi discovery and target verification

- [ ] Outside `HERDR_ENV=1`, Proqi remains a normal scratchpad and exposes no
  direct-submit target.
- [ ] Proqi negotiates the supported Herdr client/server protocol and verifies
  the `agent.prompt` request plus `agent_prompted` receipt schemas.
- [ ] Discovery validates source and target pane IDs, distinct panes, matching
  workspace, matching tab, agent kind, readiness, geometry, edge overlap, and
  session identity where required.
- [ ] A neighbor returned by directional lookup is independently present in the
  agent list and layout snapshot.
- [ ] Self-targets, cross-tab targets, cross-workspace targets, non-overlapping
  panes, invalid geometry, duplicate identities, and ambiguous neighbors fail
  closed.
- [ ] An established candidate appears exactly once with the correct direction,
  harness label, and display name.
- [ ] Initial unsupported discovery is silent. Explicit refresh explains why
  submission is unavailable without disturbing the board.
- [ ] Host focus gained refreshes targets after a harness starts, exits, or is
  replaced while Proqi keeps running.
- [ ] Resize bursts refresh geometry after settling without losing thought
  focus, cursor, selection, or valid scroll bounds.
- [ ] Every submission revalidates the complete target immediately before the
  semantic prompt call.

## 3. Required user stories

### Capture and navigation baseline

Harness work must not regress the scratchpad flow used to prepare a prompt.

- [ ] `n`, Enter on `+ New thought`, paste in board mode, and the insertion-row
  mouse target each create one thought and enter the expected focus or edit
  state.
- [ ] On an empty board, two consecutive unshifted Down or configured
  next-thought commands create one durable blank and enter edit mode.
- [ ] From the final thought, one Down focuses `+ New thought`; two further
  consecutive Down or configured next-thought commands create one durable blank
  and enter edit mode.
- [ ] An unrelated command, Escape, Up, or reorder input resets the insertion
  confirmation and does not create a thought accidentally.
- [ ] Repeated downward movement while editing the same empty blank does not
  create additional thoughts.
- [ ] Exact multiline and Unicode prompt content remains durable through edit,
  board mode, resize, harness refresh, and submission targeting.

### One adjacent harness

- [ ] One eligible adjacent candidate exposes `s Submit` and
  `S Submit & keep` without a target-selection step.
- [ ] `Submit & keep` sends the exact thought and always preserves it.
- [ ] `Submit` removes an unchanged thought only after a matching accepted
  receipt.
- [ ] The successful deletion is durable and undoable.
- [ ] Editing the thought while delivery is in flight prevents its removal.
- [ ] Failure, timeout, ambiguity, rejection, target change, or receipt mismatch
  preserves the thought and reports the failure.
- [ ] Proqi records acceptance without waiting for or reading the harness's
  answer.

### Multiple thoughts

- [ ] Selected thoughts are submitted once in board order.
- [ ] Their exact contents are separated by one blank line.
- [ ] `Submit & keep` preserves every selected source.
- [ ] `Submit` removes all unchanged sources as one undoable operation only
  after acceptance.
- [ ] Any failed or ambiguous outcome preserves every source.

### Multiple adjacent harnesses

- [ ] With two or more eligible agents, Proqi never guesses a destination.
- [ ] `s` and `S` enter directional targeting while preserving their remove or
  keep disposition.
- [ ] Arrow keys and `h`, `j`, `k`, and `l` route only to enabled directions.
- [ ] Escape cancels targeting without submitting or changing the thought.
- [ ] Mouse selection of an agent indicator routes to that exact target.
- [ ] Narrow and shallow panes keep the chooser understandable and operable,
  even when control labels must compact.

### Mixed-harness row

Use the candidate in both positions of these layouts:

```text
Claude | Proqi | <kind>
<kind> | Proqi | Codex
```

- [ ] Proqi shows both verified agent kinds with the correct left/right arrows.
- [ ] Submit enters the direction chooser rather than preferring Codex, Claude,
  the candidate, the idle agent, or the most recently discovered agent.
- [ ] Left/`h` reaches only the left harness.
- [ ] Right/`l` reaches only the right harness.
- [ ] Each harness receives the marker exactly once and returns independent
  lifecycle and session metadata.
- [ ] Replacing Claude with the candidate, and the candidate with Codex, while
  Proqi remains open refreshes the controls without restarting Proqi.

### Four directions

Build this live layout with verified agents on every side:

```text
          <up>
<left> | Proqi | <right>
         <down>
```

- [ ] Herdr reports a distinct, edge-overlapping neighbor for up, right, down,
  and left.
- [ ] Proqi renders `↑`, `→`, `↓`, and `←` targets.
- [ ] Submit exposes four directional choices.
- [ ] `Submit & keep` sends one marker upward and only the upper agent receives
  it.
- [ ] `Submit & keep` sends one marker rightward and only the right agent
  receives it.
- [ ] `Submit & keep` sends one marker downward and only the lower agent
  receives it.
- [ ] `Submit & keep` sends one marker leftward and only the left agent receives
  it.
- [ ] Every direction returns a matching accepted receipt and the source thought
  remains present.

Recommended harmless live marker:

```text
Reply with exactly: PROQI_<KIND>_<DIRECTION>_OK
```

## 4. Session creation and empty-harness behavior

### Required default for a new harness

- [ ] The harness reports its stable session identity before Proqi considers it
  eligible.
- [ ] If the harness is interactive but has not reported a session, Proqi hides
  it rather than weakening target identity.
- [ ] The target appears automatically after the session hook reports a valid
  identity and Proqi refreshes discovery.

Proqi deliberately has an open string contract for established agent kinds.
Do not add a closed enum merely to recognize a new established-session harness.

- [ ] Confirm the generic established-session path works without a
  harness-specific Proqi branch.
- [ ] If product logic truly branches on the new kind, add one canonical
  `*_AGENT_KIND` constant and use it throughout typed code. Keep literal values
  in raw JSON or shell fixtures when those fixtures are proving the wire format.

### Conditional: a harness cannot identify a session before its first prompt

This is not automatically supported. Codex, Cline, Kilo, and OpenCode are the
current explicit exceptions. Do not extend provisional eligibility to another
harness implicitly; record its replacement identity guarantee and remaining
limitation explicitly.

- [ ] Document why the harness cannot report a stable session before input.
- [ ] Prefer fixing the Herdr integration or harness hook so the session is
  reported before the first prompt.
- [ ] If provisional support is still proposed, document the identity guarantee
  that replaces the missing session ID.
- [ ] Revalidate the same pane, workspace, tab, direction, geometry, and harness
  kind immediately before delivery.
- [ ] Accept only a matching semantic receipt; never fall back to raw key
  injection.
- [ ] Cover both valid receipt timings: the first receipt already contains the
  new session, or the receipt precedes the session hook.
- [ ] When the receipt precedes the hook, record success and immediately
  rediscover identity without resending the prompt.
- [ ] Confirm the first prompt arrives exactly once and the thought disposition
  follows the accepted receipt.
- [ ] Confirm later submissions use the established session identity.
- [ ] An established target whose receipt loses or changes session identity
  fails closed.
- [ ] Explicitly document any remaining replacement race. Under Herdr protocol
  19, a same-pane, same-kind sessionless replacement cannot be detected without
  a stable pre-session instance ID or an atomic expected-instance precondition.

## 5. Switching, races, and failure safety

- [ ] Switch from an established harness to the candidate in the same pane while
  Proqi remains open; controls refresh to the new kind and session.
- [ ] Switch from the candidate to another harness in the same pane while Proqi
  remains open; the old target disappears and the new target appears.
- [ ] A session or pane replacement observed before submission fails as
  `target changed` and sends nothing.
- [ ] A replacement after Proqi's revalidation but before Herdr accepts the
  prompt is treated as an integration race: a mismatched receipt preserves the
  source, but protocol 19 cannot guarantee that the replacement received no
  text without an atomic expected-instance precondition.
- [ ] A different session in the receipt is rejected even when pane and harness
  kind still match.
- [ ] A receipt for another submission ID, pane, tab, workspace, direction, or
  harness kind is rejected.
- [ ] Volatile changes to readiness, display name, or geometry after delivery do
  not invalidate an otherwise matching accepted receipt.
- [ ] A crash after preparing but before sending becomes `cancelled` during
  recovery and is never auto-retried.
- [ ] A crash after sending but before recording the outcome becomes
  `outcome_unknown` and is never auto-retried.
- [ ] Concurrent submission limitations are documented: Herdr protocol 19 does
  not guarantee a distinct turn boundary when senders overlap.

## 6. Privacy, durability, and process quality

- [ ] Prompt content is absent from integration metadata and the submission
  journal; only ordered source IDs and redacted digests are stored.
- [ ] Raw pane and agent session identifiers are absent from the journal target
  fingerprint.
- [ ] No SQLite transaction remains open while invoking Herdr or the harness.
- [ ] One active attempt locks each source thought against mutation paths that
  would violate submission semantics.
- [ ] Integration commands use argument vectors or standard input and never a
  shell-interpolated prompt.
- [ ] No error message, diagnostic log, snapshot, or fixture exposes private
  credentials or conversation content.
- [ ] Terminal setup, cancellation, process ownership, and cleanup remain
  panic-safe and bounded.
- [ ] Copy and cut remain available when the Herdr integration or candidate
  harness is unavailable.
- [ ] Proqi never injects raw terminal keys as a delivery fallback.

## 7. Automated evidence

- [ ] Add or update recorded Herdr JSON fixtures for detection, established
  identity, readiness, prompt receipt, and exit.
- [ ] Add discovery tests for valid and invalid identity, readiness, workspace,
  tab, geometry, edge overlap, ambiguity, and all four directions.
- [ ] Add submission tests for exact payload, accepted receipt, mismatched
  receipt, timeout, rejection, malformed output, and target replacement.
- [ ] If session timing can vary, test receipt-with-session and
  receipt-before-session-hook paths separately.
- [ ] Add UI tests for a single target, multiple targets, mixed harness kinds,
  keyboard direction choice, mouse direction choice, keep, remove, failure,
  and in-flight source changes.
- [ ] Add deterministic narrow, wide, tall, shallow, and resize coverage.
- [ ] Review representative Insta snapshot diffs for any visible harness label
  or control change; leave no `.snap.new` files.
- [ ] Run focused adapter and UI tests while developing.
- [ ] Run `cargo xtask check` immediately before commit.
- [ ] At a release milestone, run `cargo xtask audit` and
  `cargo xtask package` as required by the repository contract.

## 8. Live Herdr smoke evidence

- [ ] Verify `HERDR_ENV=1` before issuing control commands.
- [ ] Use an isolated test tab or only panes explicitly created for the smoke
  test. Record every opaque pane ID from Herdr responses.
- [ ] Start the real candidate through `herdr agent start`; do not simulate its
  identity with display metadata.
- [ ] Capture `herdr agent get` evidence before the first prompt, while working,
  after completion, and after exit.
- [ ] Verify the exact marker in the harness detection or recent-unwrapped
  buffer when the alternate screen permits it.
- [ ] Verify Proqi's accepted status independently of the harness's later
  answer.
- [ ] Exercise the one-target, mixed-row, four-direction, switching, and
  applicable empty-session stories above.
- [ ] Exit agents normally, close only panes created by the test, remove test
  thoughts, and restore the user's original focus and layout.
- [ ] Confirm `git status --short` contains no runtime evidence or unintended
  repository changes.
- [ ] The qualification record names every unsupported scenario instead of
  presenting partial support as complete.
