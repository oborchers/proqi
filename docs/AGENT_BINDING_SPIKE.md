# Persistent agent binding spike

Status: decision-ready research spike, no implementation

Studied Proqi revision: `fcc2466211058f39586f6d69143f5c6246efc3f3`

Studied Sendoff revision: [`e90af467be9541796af99600b1d484a8e5e82172`](https://github.com/exviolet/sendoff/tree/e90af467be9541796af99600b1d484a8e5e82172)

Research date: 2026-09-04

## Executive recommendation

Proceed, in stages, with a Herdr-first product validation. Do not start by building a generic native-provider socket foundation.

The product invariant is sound:

> One Proqi session has zero or one persistent binding to one exact coding-agent session.

The unique value is a trustworthy recurring destination for a board. A user can keep composing and recovering prompts in Proqi, restart Proqi, prove the same agent again, and deliver in one action without turning the board into a terminal multiplexer or weakening source safety.

The safest sequence is:

1. Land the planned behavior-neutral split of adjacent discovery, delivery, pane presentation, and application submission policy.
2. Add a provider-neutral target catalog shape, migrate the journal route before its first use, and implement only global Herdr discovery and keep-only one-off delivery.
3. Add a stable Herdr server namespace plus a discoverable authenticated standalone control locator, migrate the binding preference before its first use, then validate Link to agent with established Herdr sessions, exact session identity, fresh route proof, quiet status, and explicit stale handling.
4. Generalize the proven product model into durable `BoundSessionAddress` and live connection abstractions.
5. Qualify a Pi extension as the first native endpoint. Require attributable acceptance, incarnation proof, and durable submission idempotency before remove-after-success is enabled.
6. Defer Hermes, Codex, and Claude Code production adapters until their ordinary live-session surfaces satisfy the same contract.

This ordering is stronger than building all-provider infrastructure first because it resolves the largest product uncertainties with the existing structured Herdr route. It also avoids blessing provider APIs that currently expose transcripts, resumable history, or process-local control without a safe attach contract.

Global Herdr binding is suitable for validating the interaction model, not for proving the final provider-neutral assurance ceiling. Protocol 19 proves that Herdr admitted text for the revalidated agent's terminal route. It does not prove a distinct user-turn boundary, provider acceptance, durable provider queueing, provider-side idempotency, or model processing. New global and linked Herdr routes must therefore be keep-only until a stronger Herdr protocol proves a qualifying outcome. Existing adjacent removal is compatibility debt. This spike recommends that the lower-case adjacent action also retain its source when typed assurance becomes authoritative, but that behavior change requires Oliver's explicit product decision.

## Decisions and boundaries

### Confirmed product model

The binding belongs to the Proqi session, which is the durable board. It does not belong to a pane, process, thought, provider installation, harness kind, project directory, or mutable display label.

The target is an exact durable harness session. `Codex`, `Claude Code`, or `Pi` alone is insufficient because several sessions of the same harness can be live in the same project, can share a display name, and can be replaced independently. A provider kind answers how to connect. A durable agent session identity answers where the user's prompts belong.

Zero or one is the correct first invariant because it keeps the default action legible, the status projection singular, and takeover semantics auditable. Multiple simultaneous destinations would introduce fan-out receipts, partial success, mixed removal eligibility, and ambiguous ownership. Those are different products and are outside this spike.

### Explicitly outside this scope

- No binding, adapter, migration, protocol, UI, or status implementation.
- No broadcast or multi-agent fan-out.
- No terminal input injection as a native-provider fallback.
- No label-based repair, last-target fallback, or adjacent-pane fallback.
- No claim that a Proqi connection excludes a harness UI, SDK, or uncooperative local client.
- No merging of persistent agent binding with Shared Proqi sessions.
- No implementation change to the current adjacent `s` and `S` paths in this spike.

### Open product decisions for Oliver

The spike recommends the following answers, but these are product choices that should be ratified before implementation:

1. Inside Herdr, preserve adjacent route precedence for `s`, `S`, Primary+Enter, and Primary+Shift+Enter. Recommended, subject to explicit approval: once assurance is typed, make terminal-admitted outcomes retain their source even for the lower-case remove request, with an explicit `terminal input queued; source kept` result. A stronger Herdr receipt can restore actual removal eligibility.
2. Outside Herdr, let Primary+Enter submit-and-remove only when the bound route advertises qualifying assurance. Let Primary+Shift+Enter submit and keep. A protocol 19 Herdr binding supports only the latter. If no binding or qualifying mode is ready, keep the source and offer Link, Reconnect, or the available keep action.
3. Require confirmation when an exact durable session resumes as a new live incarnation. A transport refresh to the already-proven incarnation may be silent.
4. Do not enable remove-after-success for a native provider until its receipt is attributable and its submission identity is durably idempotent. A qualified keep-only preview may precede that gate.
5. Treat Claude Code Channels as a research candidate, not a production attach contract, until the plugin, organization, authentication, receipt, and deduplication constraints are proven.
6. Add a restrained Link to agent affordance directly below `+ New thought` when the empty board is unlinked. Keep the status action as the compact persistent entry point.

### Settled technical decisions

- Durable preference and live connection are separate types and lifecycles.
- Identity uses provider scope plus exact durable harness session ID. Labels, paths, PIDs, recency, and endpoint locators never repair identity.
- The target catalog is canonical for one-off Send and persistent Link, while the two chooser intents have distinct result types.
- Every send revalidates durable identity, live incarnation, endpoint ownership, protocol, capabilities, binding ownership, and generation.
- Existing submission assembly, attachment preflight, locks, journal, receipt matching, accepted-only removal, changed-source retention, recovery, undo, and redo remain the only delivery safety owners.
- Terminal admission is a typed non-removing outcome. No adapter can promote it to provider acceptance.
- Every asynchronous connect, status, and delivery result is generation checked, bounded, cancellable, and stale safe.
- A current-user Proqi lease coordinates only cooperating Proqi connections.

### Difficult-to-reverse commitments

- Serialized address discriminants and fingerprint versions.
- The exact meaning of provider acceptance, durable queueing, and terminal admission.
- Primary+Enter precedence inside and outside Herdr.
- Whether a resumed session with a new incarnation reconnects automatically.
- Board-wide binding ownership versus attached-view adjacent routing.
- Provider protocol families, capability spelling, takeover semantics, and dedupe scope.
- Status vocabulary and the distinction between disconnected, stale, incompatible, and outcome unknown.
- Diagnostic redaction guarantees and durable lifecycle-operation recovery.

## Current Proqi baseline

### Adjacent Herdr delivery

Current ownership is spread across [`src/ports/agent.rs`](../src/ports/agent.rs), [`src/adapters/herdr`](../src/adapters/herdr), [`src/ui/app/agent_preparation.rs`](../src/ui/app/agent_preparation.rs), and [`src/ui/app/agent_delivery.rs`](../src/ui/app/agent_delivery.rs).

The current route has valuable properties:

- Herdr protocol and schema are checked before discovery.
- Discovery uses the current pane, layout, all four neighbors, agent list, session metadata, and readiness.
- Targets are revalidated immediately before submission.
- Established targets include the durable harness session identity.
- Codex, Kilo, and OpenCode may appear provisionally before a session identity exists. Those provisional targets are useful for adjacent delivery but are not eligible for a persistent link.
- An exact `agent_prompted` receipt is matched to the target identity under the existing adjacent contract.
- A same-kind sessionless replacement is rejected rather than trusted.
- Discovery has a 3 second command timeout and submission has a 5 second command timeout.

Current limitations matter to this design:

- `AgentGateway` looks generic at the trait boundary, but its target and discovery vocabulary is adjacent-pane specific.
- Direction, source pane, target pane, workspace, tab, and Herdr presentation are mixed into the route model.
- The UI currently constructs target hashes and submission requests that should become application-owned policy.
- A Herdr receipt proves terminal-route text admission. It is not provider acceptance, a provider-durable queue acknowledgement, distinct turn proof, idempotency record, or completion event.
- There is no provider process incarnation or generation in the current stable target identity.
- Herdr does not provide global exclusivity over a harness session.

### Submission safety owners to reuse

The existing application and storage path already owns the difficult source-safety rules. Binding must route through these owners rather than creating a second submission path.

1. Capture exact ordered source identities and revisions.
2. Assemble multiple thoughts with one blank line and the existing shared command-starter rule.
3. Run attachment health and preflight before journaling delivery.
4. Lock every source from accepted intent through a terminal journal state.
5. Persist the attempt as `prepared`, then transition to `sending` before the external call.
6. Match the receipt to the exact target fingerprint.
7. Mark accepted and optionally remove only unchanged sources in one durable transaction.
8. Retain changed sources.
9. On restart, cancel `prepared`; convert `sending` to `outcome_unknown`; never retry automatically.
10. Record accepted removal as a normal Board operation so undo and redo remain authoritative.

The current schema is version 12 and the storage protocol is version 11. `submission_attempts` currently encodes adjacent direction and a target fingerprint. Any future route generalization is a forward migration and mixed-version protocol change, not a reinterpretation of those rows.

### Session, ownership, and UI baseline

- A Proqi session already has a stable identifier, name, working-directory context, durable SQLite history, and one active process owner.
- Attached views and owner coordination already distinguish durable board state from view-local presentation.
- The footer can project several responsive status bands and already has compact and hidden layouts.
- Commands, Help, labels, layout, and hit testing are intended to derive from shared semantic definitions.
- The command palette is an application entry point, not an alternate delivery engine.
- Update coordination and storage ownership already refuse unsafe mixed writers.

### Similar-looking features that are not agent binding

Invocation discovery is authoring assistance. A discovered invocation inserts inert collaborator syntax into the current thought. It does not identify a live delivery endpoint, reserve an agent, focus a pane, or submit anything.

Cross-session thought transfer copies thoughts between two Proqi sessions. Its destination receipt is a Proqi durability receipt, not an agent acceptance receipt. A board-wide agent binding must never be inferred from a transfer destination.

An attached view's adjacent Herdr target is local routing context for that view. It is not the board's persistent agent binding. Shared Proqi sessions will need to carry the board-wide preference as authoritative session state while keeping adjacent topology view-local.

### Planned prerequisites and adjacent roadmap work

The roadmap correctly places a behavior-neutral boundary split before global or persistent delivery. That work should land first and preserve current public paths, CLI and JSON spelling, durable encodings, error classifications, and snapshots.

Global Herdr delivery should then establish the shared target catalog and one-off chooser. Persistent Link should reuse that catalog. The separate Shared Proqi sessions spike informs owner and attached-view coordination, but it must not absorb binding lifecycle, provider authentication, or agent delivery semantics.

The only active feature pull request found during this spike was draft PR 48, smart paste reflow. It touches editing and persistence lanes but does not define agent identity or delivery. Open issues 52 and 56 concern terminal input and Home/End behavior. No open agent-binding implementation was found.

## Sendoff prior art

Sendoff was cloned into a unique disposable directory and studied at exact commit [`e90af467be9541796af99600b1d484a8e5e82172`](https://github.com/exviolet/sendoff/tree/e90af467be9541796af99600b1d484a8e5e82172). The clone was used read-only and removed after the spike.

### Relevant sources

- [`terminalTargets/types.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/lib/terminalTargets/types.ts) defines discriminated persisted bindings, live targets, resolution, and provider operations.
- [`useTerminalActions.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/hooks/useTerminalActions.ts) implements explicit binding, last-target, then picker precedence.
- [`lastTargetStore.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/store/lastTargetStore.ts) keeps the last target only in memory.
- [`TargetPicker.tsx`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/components/TargetPicker/TargetPicker.tsx) shares sectioned provider discovery between send and bind modes.
- [`useTargetStatus.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/hooks/useTargetStatus.ts) polls the active bound tab every 3 seconds.
- [`herdr.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/lib/terminalTargets/herdr.ts), [`orca.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/lib/terminalTargets/orca.ts), and [`tmux.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/lib/terminalTargets/tmux.ts) expose the concrete resolution and injection behavior.
- [`herdrResolve.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/lib/herdrResolve.ts) contains stable-ID and label-repair behavior.
- [`targetResolve.test.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/lib/targetResolve.test.ts) proves exact lookup, replacement rejection, unique-label repair, ambiguity, and tmux parsing.
- [`editorStore.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/store/editorStore.ts) and [`db.test.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/lib/db.test.ts) cover persisted per-tab binding.
- [`diagnostics/collect.ts`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/lib/diagnostics/collect.ts) and [`DoctorModal.tsx`](https://github.com/exviolet/sendoff/blob/e90af467be9541796af99600b1d484a8e5e82172/web/src/components/Doctor/DoctorModal.tsx) implement source-specific support diagnostics.

The abstraction entered through `be1e085` and the later diagnostics work through `53f10a0`. The studied repository is MIT licensed.

### Ideas to retain

- One persistent target per editor-like unit.
- A single action after a destination is proven.
- One sectioned target catalog for one-off send and persistent link.
- Link selection that does not itself deliver.
- Explicit not-found and ambiguous results that return to a chooser.
- Quiet status limited to the active bound context.
- Independent provider discovery so one failed provider does not erase healthy providers.

### Semantics to reject

Sendoff is an interaction reference, not an assurance authority.

- It repairs Herdr and tmux bindings through unique mutable labels. Proqi must match only exact durable session identity.
- It falls back from an explicit binding to a last target and then a picker. Proqi must fail closed for stale persistent bindings.
- Orca and tmux delivery use timed terminal input injection. Queued terminal bytes do not prove a semantic prompt boundary.
- Successful process exit is treated as sent. Proqi requires an attributable typed receipt.
- Missing, ambiguous, or failed status polling becomes no status. Proqi must show that freshness is unknown or action is required.
- It has no Proqi-style durable attempt journal, source revision check, accepted-only removal, or outcome-unknown recovery.

### Disposable experiment

Oracle: pure resolution tests should prove whether exact ID matching rejects replacement, whether label repair is possible, and whether ambiguity is explicit. They do not prove persistence or delivery acceptance.

`bun test web/src/lib/targetResolve.test.ts` passed 15 tests. It proved exact ID and label checks, rejected a reused ID with conflicting identity, self-healed a unique label match, returned ambiguity for duplicate matches, and exercised tmux parsing. Proqi rejects the self-healing label rule even though the test proves it is intentional in Sendoff.

The frozen dependency install could not be run in the sandbox because Bun was denied access to its temporary directory. The requested escalation was rejected because package installation scripts exceeded the read-only research scope. Persistence and adapter delivery were therefore inspected statically, not executed.

## Provider capability matrix

`Yes` means the ordinary currently studied integration surface exposes the property. `Extension` means Proqi could add it only through a cooperating provider component. `No` means historical state, transcript enumeration, terminal injection, or a second process does not qualify.

| Capability | Herdr | Pi | Hermes | Codex | Claude Code |
| --- | --- | --- | --- | --- | --- |
| Supported live discovery | Yes, panes plus agent list | Extension | Process-local gateway only | Embedded app-server only | Live enumeration exists; Channels require an enabled running session |
| Stable durable session identity | Yes for established harness sessions | Yes | Yes | Yes, thread UUID | Yes, session UUID |
| Live incarnation or generation | Incomplete | Extension | Gateway process implicit, not published globally | Server process implicit, not published globally | Extension or channel instance needed |
| Semantic prompt boundary | No; `agent prompt` is structured terminal admission | `sendUserMessage` in an extension | Gateway JSON-RPC submit or steer | `turn/start` or `turn/steer` | Channel message into enabled session |
| Attributable acceptance or durable queue receipt | No; `agent_prompted` is terminal-route admission | No, current example returns void | JSON-RPC response, durability not established | Response and `clientUserMessageId` correlation, no documented durability | Channel transport receipt is not documented provider acceptance |
| Provider-side idempotency | No | No | No documented submission dedupe | No documented submission dedupe | No documented submission dedupe |
| Idle, steer, follow-up, queue | Readiness states; delivery mode not typed | Idle, steer, follow-up | Submit, steer, and process-local queue behavior | Start, steer, experimental queued items | Channel follow-up into a running session; exact modes need proof |
| Exclusive binding or takeover | No | Extension | No ordinary global contract | No | No ordinary global contract |
| Completion or blocked events | Polled Herdr readiness | Extension events | Gateway event stream | App-server status and turn events | Hooks, agents output, or plugin events need correlation |
| Compatibility risk | High until server namespace, standalone locator, and stronger receipt versions exist | High; extension API is the compatibility boundary and needs per-release qualification | High; ordinary TUI and hosted gateway are different products | High; embedded server and queue surfaces are experimental or process-local | High; Channels are preview, allowlisted, authentication-sensitive, and plugin-dependent |
| Current viability | Managed one-off UX viable now; persistent standalone UX needs namespace and locator | Best native extension candidate | Defer ordinary TUI; hosted gateway is a distinct profile | Blocked on discoverable attach endpoint and stronger receipts | Research preview candidate only |
| Recommended priority | 1 | 2 | 4 | 5 | 3 for research, not production |

### Herdr

Herdr is the only route already integrated end to end. Current discovery is nevertheless managed-pane only: [`src/adapters/herdr/mod.rs`](../src/adapters/herdr/mod.rs) constructs the gateway from inherited managed context, and [`src/adapters/herdr/discovery.rs`](../src/adapters/herdr/discovery.rs) refuses discovery without `HERDR_ENV`. `Global` means all agents on that current Herdr server. It does not mean that standalone Proqi can locate or authenticate a Herdr server.

Persistent links must exclude provisional sessionless targets and resolve established sessions by exact harness session ID within an exact Herdr server scope. Protocol 19 exposes neither a stable server namespace nor a discoverable authenticated standalone locator. Persistent Herdr Link and outside-Herdr shortcuts require both prerequisites. The locator may publish a user-only transient endpoint reference, but the durable preference stores only the server namespace. Every Proqi restart rediscovers and authenticates the endpoint. Global one-off discovery in Stage 1 remains managed-pane only and explicitly scoped to the current server. A moved pane can be rediscovered after the server namespace and durable session ID both match. A matching label cannot repair a missing identity.

Managed Herdr can validate the global chooser before those prerequisites. With the namespace and standalone authenticated locator, Herdr can then validate preference persistence, standalone restart, stale status, explicit reconnect, routing policy, journaling, and source retention. Adjacent, global, and linked Herdr routes are `TerminalBytesQueued` under protocol 19. They cannot validate native-provider acceptance, idempotency, durable queue receipts, or remove-after-success. Existing adjacent behavior remains unchanged during the behavior-neutral split. If Oliver ratifies the assurance migration, a later unqualified lower-case removal request retains its source and reports why.

### Pi

The earlier spike studied Pi 0.84.3. Current public source was checked at [`17de82d7bea18a6589677a9761baabc2060c9efb`](https://github.com/earendil-works/pi/tree/17de82d7bea18a6589677a9761baabc2060c9efb). Its [`send-user-message.ts`](https://github.com/earendil-works/pi/blob/17de82d7bea18a6589677a9761baabc2060c9efb/packages/coding-agent/examples/extensions/send-user-message.ts) demonstrates idle, steer, and follow-up semantics, but returns no acceptance value and supplies no global registry, live incarnation, Proqi lease, or durable dedupe.

Pi remains the best extension candidate because a first-party extension can own those missing surfaces. It is not currently qualified merely because sessions have durable UUIDs. Pi was not installed on the research machine, so no private or live session experiment was attempted.

### Hermes

The earlier spike studied Hermes 0.20.6. Current public source was checked at [`b0ab2e163a50d4e6c36507eba955a6067fde6abc`](https://github.com/NousResearch/hermes-agent/tree/b0ab2e163a50d4e6c36507eba955a6067fde6abc). The [programmatic integration guide](https://github.com/NousResearch/hermes-agent/blob/b0ab2e163a50d4e6c36507eba955a6067fde6abc/website/docs/developer-guide/programmatic-integration.md) now documents ACP and a TUI gateway over stdio or WebSocket. Active sessions are process-local to that gateway. The ordinary TUI still lacks a discoverable, authenticated attach registry and a published live generation.

A Proqi-managed Hermes gateway would be a separate hosted profile, not attachment to an arbitrary existing TUI. That option can be evaluated later without calling ordinary Hermes binding feasible today. Hermes was not installed locally.

### Codex

The earlier spike studied Codex 0.150.1; local read-only inspection found 0.153.2. Current public source was checked at [`9c4253ffc1b954337bf2f494aadc55e9cd132a48`](https://github.com/openai/codex/tree/9c4253ffc1b954337bf2f494aadc55e9cd132a48). The [app-server contract](https://github.com/openai/codex/blob/9c4253ffc1b954337bf2f494aadc55e9cd132a48/codex-rs/app-server/README.md) has thread UUIDs, thread status, `turn/start`, `turn/steer`, expected-turn checks, and correlation via `clientUserMessageId`. It does not document that the ID deduplicates a repeated submission.

An ordinary Codex TUI embeds its control surface. Starting another app-server can resume history but is not proof that Proqi is controlling the already-running agent. Production viability requires a discoverable shared endpoint, live server incarnation, peer authentication, session binding, attributable acceptance, and idempotency.

### Claude Code

The earlier spike studied Claude Code 2.1.251; local read-only inspection found 2.1.260. Since that spike, official [Channels documentation](https://code.claude.com/docs/en/channels) exposes a research-preview path for pushing messages into a running session. Channels require prelaunch opt-in, organization or allowlist conditions, supported authentication, and a cooperating plugin. They are not a general control endpoint for any ordinary running session. The [Channels reference](https://code.claude.com/docs/en/channels-reference) also does not establish Proqi-grade durable acceptance or deduplication.

This moves Claude Code from an unavailable upstream concept to a worthwhile qualified experiment. It does not make it production-feasible. A disposable plugin spike must prove exact session identity, channel incarnation, authentication, acceptance correlation, dedupe, status, teardown, and behavior when the session is no longer channel-enabled.

## Product flows and routing

### Actions

| Action | Exact meaning | Content effect |
| --- | --- | --- |
| Link to agent | Discover, choose, validate, bind, then durably remember one exact session | None |
| Change agent | Establish a different exact live binding, then atomically replace the preference | None |
| Reconnect | Resolve only the existing durable address and prove a new live route | None |
| Disconnect | Release the live cooperating connection while retaining the preference | None |
| Unlink | Clear the preference and boundedly release the cooperating connection | None |
| Take over | Explicitly request provider-authoritative transfer from another cooperating Proqi connection | None until a later explicit send |
| Send once | Deliver through a freshly validated chosen route without changing the binding | Journal and optional source removal follow normal policy |

Change is strictly prepare-then-swap. The old binding remains active until the new provisional bind succeeds and the preference swaps atomically. A provider that requires releasing the old target first is incompatible with Change. The user may instead choose the separately confirmed Unlink action and later Link a new target, accepting the explicit disconnected interval. Change itself never silently falls back to unlinked.

Disconnect retains the preference and invalidates the live connection generation. Unlink clears the preference through a durable tombstoned lifecycle operation. A provider lease must expire independently if its idempotent release cannot be confirmed. Proqi must not leave a reusable token or live route on disk.

Takeover is available only when the provider can identify a current cooperating owner and transfer or revoke its lease. An OS user lock alone can coordinate Proqi processes, but it cannot justify a global exclusivity claim.

### Shared chooser, separate intents

One target catalog and chooser should serve `Send once` and `Link to agent`. The chooser has an explicit mode in its title, primary button, accessible label, Help text, and result type. Keyboard navigation, pointer movement, and a row click change highlighted selection only. They have no external or durable side effect.

- In Link mode, explicit activation of the labelled Link button or confirmed Enter action starts validation and binding for the highlighted row. It never sends, focuses, reserves, or mutates a thought.
- In Send mode, explicit activation of the labelled Send button or confirmed Enter action returns the highlighted exact route to the normal submission coordinator. Source capture and preflight occur through existing owners.
- Changing modes requires an explicit command. A single selection must never have different effects because content happens to be selected.

### Routing decision table

| Context and action | Route | Failure behavior |
| --- | --- | --- |
| Herdr, `s` | Current adjacent target, request remove | Recommended after explicit approval: under protocol 19, send once and report `terminal input queued; source kept`; until then current legacy removal remains |
| Herdr, `S` | Current adjacent target, keep | Preserve current error and source retention |
| Herdr, Primary+Enter | Current adjacent target, request remove | Binding does not override adjacent muscle memory; assurance still gates removal |
| Herdr, Primary+Shift+Enter | Current adjacent target, keep | Binding does not override adjacent muscle memory |
| Outside Herdr, Primary+Enter, binding connected through standalone locator | Exact bound target, remove only if assurance qualifies | Protocol 19 Herdr fails before delivery and offers keep; any validation failure retains content |
| Outside Herdr, Primary+Shift+Enter, binding connected through standalone locator | Exact bound target, keep | Fail closed, retain, offer Reconnect if validation fails |
| Outside Herdr, shortcut, no binding | No delivery | Keep composition, offer Link |
| Any context, Submit to linked agent | Exact bound target, requested disposition preserved | Never reinterpret remove as keep; never fall back to adjacent or last-used target |
| Any context, Send once | Explicitly chosen exact target | Never change the persistent binding |
| Command palette adjacent submission | Existing adjacent route | Same result as its keyboard spelling |
| Submit all to linked agent | Exact bound target through normal assembly | Same preflight, lock, journal, and assurance rules |

The persistent preference does not affect all submissions. It affects explicit linked-target actions and the standalone Primary+Enter aliases. This makes the destination visible at the moment policy is chosen and preserves existing Herdr fast paths.

### Composition when delivery is unavailable

Composition never becomes unavailable because an agent provider is missing.

- No provider installed: keep editing; Link opens an empty catalog with provider-specific availability reasons and Connection Doctor access.
- No compatible live session: keep editing; show no compatible targets and the exact capability requirement.
- Stale binding: keep editing; linked send is disabled; offer Reconnect, Change, and Unlink.
- Reconnect failure: retain content and selection; show the durable preference separately from the failed live proof.
- Provider connected but unable to accept the requested mode: disable only that delivery mode. Protocol 19 Herdr exposes keep-only; remove-after-success is unavailable and is never silently converted to keep.

## User interface specification

### Entry points

| Surface | Empty board | Populated board |
| --- | --- | --- |
| Board body | Restrained `Link to agent` row directly below `+ New thought` when unlinked | No persistent board-body chrome |
| Status row | `Link to agent` quiet action when space permits | Linked identity and state; `Link to agent` if unlinked |
| Commands | Link, Change, Reconnect, Unlink, Send once, Submit to linked agent, Submit to linked agent and keep | Same commands, filtered by state |
| Help | Explain binding, exact target identity, shortcuts, and fail-closed behavior | Same, plus current linked-target commands |
| Mouse | Click status action or linked identity to open the appropriate chooser or details | Same hit target derived from rendered geometry |
| Narrow pane | Compact `Link` or `Agent: <state>` | Hide label before state; never hide actionable failure marker |
| Shallow pane | Commands remain available even if the status band is suppressed | Same |

The empty-board primary action remains thought capture. The secondary Link row adopts the clarified roadmap placement, uses quieter styling than `+ New thought`, disappears after content is created or a binding exists, and remains duplicated in Commands and the status action. It must not visually compete with composition.

### Quiet status vocabulary

Status is a projection of persistent preference, live connection, provider activity, and delivery state. It is not a second state machine.

| State | Full projection | Compact projection | Action |
| --- | --- | --- | --- |
| Connecting | `<agent> · connecting` | `agent: connecting` | Cancel |
| Idle | `<agent> · idle` | `<agent>` | Open details |
| Working | `<agent> · working` | `<agent> · work` | Open details |
| Blocked | `<agent> · blocked` | `agent: blocked` | Open reason or provider |
| Pending delivery | `<agent> · delivering` | `agent: sending` | Cancel only before send boundary |
| Disconnected | `<agent> · disconnected` | `agent: offline` | Reconnect |
| Stale | `<agent> · link is stale` | `agent: stale` | Change or Unlink |
| Incompatible | `<agent> · update required` | `agent: incompatible` | Connection Doctor |
| Reconnect available | `<agent> · reconnect available` | `agent: reconnect` | Reconnect |
| Outcome unknown | `<agent> · delivery outcome unknown` | `agent: unknown` | Open recovery details |

Ordinary idle and working states use inherited terminal colors plus the routine forest-green focus accent. Blocked, stale, incompatible, and outcome unknown require text or a stable symbol as well as semantic color. A stale status sample must never be rendered as current. If refresh fails, the projection becomes disconnected or status unavailable with a freshness timestamp in details.

The linked display label is presentation only. Details always expose provider, harness kind, and a short safe identity suffix so two duplicate labels remain distinguishable without displaying full opaque IDs.

## Durable preference and live connection

### Durable address

The persisted value is a discriminated address, not a record of optional provider-specific fields:

```rust
enum BoundSessionAddress {
    Herdr {
        server_namespace: StableHerdrServerNamespace,
        harness: HarnessKind,
        session_id: ProviderSessionId,
    },
    Native {
        provider: ProviderId,
        protocol_family: ProtocolFamily,
        harness: HarnessKind,
        session_id: ProviderSessionId,
    },
}

struct AgentBindingPreference {
    address: BoundSessionAddress,
    last_known_label: Option<SafeDisplayLabel>,
}
```

The exact serialized spelling is an implementation decision, but the discriminant and typed payload are contract requirements. Provider-scoped opaque IDs need validated length, encoding, and nonempty constructors.

The preference may persist:

- Provider and protocol family.
- Harness kind.
- Exact durable harness session identity.
- Exact stable Herdr server namespace or equivalent provider namespace, after the provider exposes one.
- A bounded, untrusted last-known display label.

The preference must not persist:

- Endpoint paths or URLs.
- Bearer tokens or credentials.
- PIDs by themselves.
- Socket handles.
- Process nonces or generations as reusable trust.
- Liveness, locks, leases, takeover state, or connection ownership.
- Full working-directory paths.
- Last-target fallback or ranking authority.

Project and working-directory context should be recomputed from the live session. If usability later requires a durable hint, persist only a bounded, explicitly untrusted project basename. Ranking should favor an exact currently linked identity, then live compatibility and stable provider ordering. It must not infer identity from recency or labels.

Protocol 19 cannot populate `StableHerdrServerNamespace`. It exposes workspace, tab, pane, harness, and harness session identity, but the global roadmap is scoped only to the current Herdr server and the adapter refuses discovery without managed context. The new namespace and authenticated standalone locator are therefore required Herdr protocol and runtime extensions, not descriptions of current capability. Until both exist, global Herdr targets are managed, one-off current-server routes and cannot become restart-safe persistent preferences or support outside-Herdr delivery. Persisting an endpoint path, installation path, workspace label, or hash of those values would not repair either gap.

### Live connection

The live connection exists in memory and is invalidated as a unit:

```rust
struct ResolvedBinding {
    preference_fingerprint: BindingFingerprint,
    incarnation: LiveIncarnation,
    negotiated_protocol: NegotiatedProtocol,
    capabilities: CapabilitySet,
    route: TransientRouteHandle,
    peer: VerifiedPeer,
    binding_proof: BindingProof,
    generation: ConnectionGeneration,
    liveness: FreshLiveness,
}
```

Adapters own endpoints, registries, peer credentials, socket framing, process metadata, and concrete event streams. Domain and application code own the validity of IDs, capability requirements, connection transitions, assurance levels, and generation checks.

### Crash-consistent lifecycle operations

Link, Change, Unlink, Disconnect, and Takeover are cross-system operations. They need a content-redacted local operation journal and provider-side idempotent operation IDs. Delivery attempts and binding operations are separate journals.

```text
prepared -> provisional_remote -> local_committed -> remote_finalized -> complete
       \-> cancelled            \-> outcome_unknown
```

Each lifecycle row stores only an operation ID, operation kind, old and new durable address discriminants and identities, their fingerprints, phase, deadline class, and redacted result. Exact durable session identity is needed for recovery but is never projected into diagnostics. The row stores no endpoint, token, live proof, or prompt.

For providers with authoritative binding:

1. Persist `prepared` locally before a remote mutation.
2. Request an idempotent provisional lease with the operation ID and a short expiry.
3. Persist the new preference, or cleared preference plus an unlink tombstone, and `local_committed` in one SQLite transaction.
4. Finalize or release remotely with the same operation ID.
5. Persist complete only after an authoritative result.
6. After a crash, freshly rediscover and authenticate the provider, then query the operation ID. Never trust a persisted route or blindly repeat Takeover.

A provisional lease that outlives a failed local commit expires. A finalized lease with a committed preference is recoverable through a fresh query. An old lease after Change or Unlink is unusable for sending once the local binding generation changes and must either be released idempotently or expire. Change may hold the old finalized lease while acquiring the new provisional lease, but Proqi routes only through the locally committed preference.

Herdr protocol 19 has no authoritative remote binding. Its cooperating connection is a user-local Proqi lease keyed by stable server namespace and durable harness session ID. Acquiring that OS-backed lease and committing the preference must follow the existing session-lease crash pattern. Process death releases the live lease. No Herdr UI or other sender is claimed to be excluded.

If the provider cannot query an operation result, any lost response after finalization, release, or takeover becomes `BindingOperationUnknown`. Proqi disables delivery for that address until a fresh authoritative ownership query succeeds or the lease expires. Takeover is never automatically retried.

### Identity changes

| Provider event | Durable preference | Live connection | User-visible result |
| --- | --- | --- | --- |
| Display rename, same durable ID | Keep; refresh label | Keep only after normal generation check | New label, no identity warning |
| Resume, same durable ID, new incarnation | Keep | Invalidate | Reconnect available; confirm first connection to new incarnation |
| Fork | Keep old address | Invalidate if old disappears | Fork is a different choice, never automatic |
| Replacement under same label | Keep old address | Invalidate | Stale; label cannot repair it |
| Session deleted | Keep until Change or Unlink | None | Stale |
| Endpoint replaced | Keep | Invalidate | Rediscover and prove exact session |
| Session switches within a live process | Keep old address | Invalidate | Stale or reconnect available only for the exact old ID |

## Connection lifecycle

### State model

Binding resolution and delivery are orthogonal. `OutcomeUnknown` is a delivery recovery state, not proof that the binding itself is stale.

```text
Unlinked
  -> Discovering -> Linking -> Connected
                         \-> Incompatible
                         \-> Disconnected

Connected
  -> Refreshing -> Connected
               \-> Disconnected -> Reconnecting -> Connected
               \-> Stale
               \-> Incompatible
  -> AwaitingTakeover -> Connected | Disconnected
  -> Disconnecting -> Disconnected
  -> Unlinking -> Unlinked

Any asynchronous result carrying an old connection generation is discarded.
```

### Transition table

| Event | Required proof | Result |
| --- | --- | --- |
| Discover | Independent provider catalog queries | Catalog entries plus per-provider completeness |
| Link selection | Exact address, peer identity, protocol, capabilities, live incarnation, binding ownership | Crash-consistent provisional bind, local commit, and finalize |
| Proqi restart | No trust carried from prior process | Resolve preference from scratch |
| Harness restart | Same durable ID plus different incarnation | Invalidate route; offer confirmed reconnect |
| Socket loss | Generation-matched failure | Disconnect; silently refresh only if the same incarnation can be proven before any delivery |
| Endpoint replacement | Peer and incarnation change | Treat as semantic reconnect, not transport reuse |
| Before every send | Revalidate session ID, incarnation, endpoint ownership, protocol, capabilities, lease, and route generation | Send or fail closed |
| Change | Fully prove replacement first | Atomically swap preference, then release old route |
| Disconnect | Generation-matched release or bounded timeout | Keep preference; invalidate route; record unknown remote release if necessary |
| Unlink | Clear preference durably and cancel route | Unlinked even if bounded remote release fails |
| Takeover | Provider-authoritative current-owner proof, idempotent operation ID, and explicit user confirmation | New generation, refusal, or binding-operation unknown |
| Mixed or downgraded protocol | Explicit negotiation result | Incompatible, never raw fallback |

### Reconnect policy

A transport refresh may be automatic only when all of these remain true:

1. The durable session ID is unchanged.
2. The live incarnation is unchanged.
3. The same authenticated endpoint owner is proven.
4. The negotiated protocol and required capabilities are not weaker.
5. No delivery request crossed the send boundary on the lost connection.

A new incarnation is semantically different even when it resumes the same durable session. The first implementation should offer Reconnect and require confirmation. Later automation may be considered only after providers expose authoritative resume semantics and status history.

No retry is permitted after bytes containing a delivery request may have reached the provider. Reconnect can restore a route, but it cannot repeat an ambiguous submission.

### Deadlines and cancellation

Initial implementation bounds should retain the current 3 second Herdr discovery and 5 second submission deadlines, then add provider-neutral ceilings:

- 3 seconds per provider discovery, 4 seconds for the first aggregate catalog result.
- 5 seconds for handshake, bind, reconnect, or takeover.
- 3 seconds for immediate pre-send revalidation.
- 10 seconds for an attributable acceptance receipt.
- Event-driven liveness where available; otherwise a 3 second poll while visible and stale after two missed intervals.
- One pre-send transport refresh only when no request bytes were sent and all silent-refresh proofs hold.
- 2 seconds total bounded local teardown, matching the existing shared runtime expectation.

These are upper bounds for the first implementation, not provider truth. Tests must use injected clocks and deterministic timeout controls. Every operation carries cancellation and a connection generation. Cancellation after the send boundary produces `OutcomeUnknown`, not `Cancelled`.

## Delivery assurance and journal evolution

### Typed outcomes

| Outcome | Meaning | Remove-after-success allowed? |
| --- | --- | --- |
| `ProviderAccepted` | The exact provider session attributable accepted the semantic user submission | Yes, when the receipt names the submission, target, incarnation, and delivery mode |
| `ProviderQueuedDurably` | The provider durably stored the exact submission with a dedupe identity | Yes |
| `TerminalBytesQueued` | A terminal accepted input bytes or paste buffer | Never |
| `CompletionObserved` | A later turn completed | Not by itself; it must follow a qualifying acceptance |
| `Rejected` | The provider authoritatively refused the request | No |
| `OutcomeUnknown` | Acceptance may have occurred but no qualifying receipt is available | No; never auto-retry |

Herdr's current `agent_prompted` receipt maps to `TerminalBytesQueued` for adjacent, global, and linked routes. Historical adjacent attempts keep their recorded accepted state and are never rewritten. New global and linked attempts are keep-only. For adjacent attempts, this spike recommends source retention after the assurance migration, conditional on explicit product approval. That compatibility change requires release notes, command and Help copy, and snapshots. It is preferable to preserving an exception that would contradict the invariant that terminal bytes never authorize source removal.

### Submission identity and provider idempotency

The provider endpoint should accept Proqi's stable `SubmissionId`, the exact content digest, the intended durable session ID, the live incarnation, and the delivery mode. A replay with the same ID and digest must return the original receipt without creating a second user turn. A replay with the same ID and a different digest must be rejected.

The dedupe record must survive provider and client crashes for at least the full Proqi attempt-recovery window. A process-local set is insufficient.

Without provider-side idempotency, Proqi can guarantee:

- One local invocation per live attempt.
- No automatic retry after ambiguity.
- Source retention for rejected and unknown outcomes.
- Exact local journaling and recovery.

It cannot guarantee that the provider did not process a request whose receipt was lost. It also cannot prevent a duplicate if a user manually resends after an unknown outcome.

### Future journal address

Replace the direction-only concept with a versioned discriminated route address while preserving legacy rows:

```rust
enum SubmissionAddress {
    AdjacentHerdrV1 { direction: Direction },
    GlobalHerdrV1 { /* content-redacted exact route identity */ },
    BoundSessionV1 { /* provider, session, and live incarnation identity */ },
}
```

The durable attempt stores:

- Address kind and version.
- A versioned fingerprint of the exact address used for that attempt.
- Provider, protocol family, and assurance class where needed for recovery.
- Existing submission ID, source IDs and revisions, source digest, content digest, state, receipt fingerprint, and error classification.

Raw endpoint paths, credentials, full provider session IDs, prompts, and process handles do not belong in diagnostics or general attempt listings. The fingerprint for a bound delivery includes the durable session address and the proven live incarnation. The preference fingerprint deliberately excludes transient route handles.

Migration should add the new representation and decode all existing rows as `AdjacentHerdrV1`. Do not rewrite old fingerprints as if they used the new algorithm. In-flight legacy `prepared` and `sending` attempts are recovered under their existing rules before any new interpretation. Bump the schema and storage protocol, use the existing exclusive migration lease and backup flow, and refuse unsafe mixed writers.

### Crash sequences

| Failure point | Durable interpretation | Source result | Retry policy |
| --- | --- | --- | --- |
| Client crash before `sending` | `prepared` becomes cancelled | Keep | User may start a new attempt |
| Client crash after `sending`, before request leaves | Indistinguishable from later send | Keep, outcome unknown | No automatic retry |
| Provider crash before acceptance | Rejected if authoritative response exists; otherwise unknown | Keep | Reconnect, then user decides |
| Provider accepts, crashes before receipt | Outcome unknown | Keep | Query by submission ID only if provider has durable dedupe ledger; never blindly resend |
| Receipt arrives, client crashes before local commit | Journal still sending, therefore unknown on restart | Keep | Reconcile only from authoritative provider ledger |
| Accepted journal and removal commit, UI crashes | Durable accepted attempt and Board operation | Removed sources reload; undo remains available | No resend |
| Crash during optional removal transaction | Whole accepted-plus-removal transaction commits or rolls back | Reload exact durable result | Retry only the local transaction if no external call occurs |

Changed-source retention, exact multi-thought assembly, attachment health, source locks, receipt matching, recovery, undo, and redo remain owned by the existing submission coordinator and store transaction.

## Architecture

### Required behavior-neutral prerequisite

The planned split should land before binding. It should create narrow owners without introducing generic utilities:

```text
Adjacent Herdr discovery  -> adjacent target facts
Herdr delivery            -> exact request and typed receipt
Pane presentation         -> direction, labels, status hints
Submission coordinator    -> capture, preflight, journal, receipt, removal
```

This split can preserve the current `AgentGateway` public path through compatibility re-exports while moving adjacent-only fields out of the durable delivery policy. It should have no CLI, JSON, storage, behavior, or snapshot change.

### Proposed inward ownership

```text
domain
  ProviderId, HarnessKind, ProviderSessionId, BoundSessionAddress
  LiveIncarnation, CapabilitySet, DeliveryMode, DeliveryAssurance
  BindingState transitions and validated fingerprints
        ^
ports
  TargetCatalogSource, SessionConnector, PromptDelivery
  BindingPreferenceStore, ConnectionLease
        ^
application
  TargetCatalogCoordinator, AgentConnectionManager
  Link/Change/Reconnect/Unlink/Takeover use cases
  SubmissionCoordinator and AgentStatusReadModel
        ^
adapters                                  UI composition
  Herdr/global discovery and delivery      chooser modes and actions
  native registries and sockets            responsive status projection
  peer credentials and framing             Commands and Help
  SQLite migration and preferences         pointer mapping and details
  provider status streams                  Connection Doctor rendering
```

The dependency direction remains `domain <- ports <- application <- adapters and UI composition`.

### Narrow ports

`TargetCatalogSource` returns one provider's entries plus truthful completeness. `TargetCatalogCoordinator` runs sources independently and produces a shared catalog for Send once and Link.

`SessionConnector` resolves a durable address, handshakes, binds, refreshes, disconnects, and optionally takes over. Its successful result contains typed live proofs, not adapter endpoints.

`PromptDelivery` accepts an already validated connection generation and a `SubmissionRequest`. It returns a typed assurance receipt whose target and submission fingerprints application code verifies.

`BindingPreferenceStore` is part of the existing storage transaction boundary. It is not a provider registry and must not bypass active-owner coordination.

`ConnectionLease` expresses one cooperating Proqi connection for one harness session. Its contract explicitly excludes global control over the harness and uncooperative clients.

### Canonical target catalog

Every provider source returns one of:

```text
Complete(entries)
Incomplete(entries, redacted reasons)
Unavailable(redacted reason)
Incompatible(required, offered)
```

Sources run independently. A failed Pi source cannot hide Herdr entries. An incomplete source cannot be presented as a complete global list. Catalog entries carry a stable address plus current display and capability facts, but selection triggers a fresh proof for Link or Send. Ranking never turns a hint into identity.

### Asynchronous status safety

- One bounded worker or shared runtime task owns each active connection.
- Every connect, status, and delivery result carries both binding fingerprint and connection generation.
- The application discards results for an older generation or changed preference.
- Event streams are preferred; polling is visibility-aware and bounded.
- Queues coalesce replaceable status updates but never delivery receipts.
- Cancellation is idempotent and teardown bounded.
- A failed refresh invalidates freshness. It never leaves an old idle state looking current.
- A connection can be live but incapable of the requested mode. The read model represents that incompatibility explicitly.

### Contract impact of a later implementation

| Contract | Expected change |
| --- | --- |
| Public Rust | New domain values and ports; compatibility path for existing adjacent types |
| CLI | Potential read-only binding/status/doctor commands; exact scope requires separate CLI design |
| JSON | Versioned discriminants, assurance, capability, and redacted reason codes if exposed |
| Control protocol | Owner-mediated link mutations and board-wide status for attached views |
| Storage protocol | Bump for binding preference and generalized attempt address |
| SQLite | Forward migration, constraints, indexes, backup, integrity tests |
| Diagnostics | Provider availability, negotiation, resolution, lease, freshness, and rejection codes |
| Configuration | Provider enablement and bounded timeout overrides, never durable endpoint trust |
| Snapshots | Footer, chooser, Commands, Help, details, error, narrow, and shallow states |
| Herdr integration | Global catalog and exact session resolution; possibly stronger protocol fields later |

### Connection Doctor

Connection Doctor is read-only and content-redacted. It should show:

- Provider installed, unavailable, disabled, or incompatible.
- Discovery complete, incomplete, timed out, or denied.
- Required and offered protocol families and capability names.
- Durable binding resolved, missing, ambiguous, replaced, or deleted.
- Peer validation and same-user authentication success or classified failure.
- Live generation changed, lease held, takeover supported, or ownership unknown.
- Last status freshness and last rejection reason code.

It must not show executable paths, socket paths, full working directories, prompts, thought content, full session IDs, bearer tokens, credentials, raw provider output, or process command lines. Exported support diagnostics use bounded counts, versions, booleans, short safe provider names, and enumerated reason codes.

## Security and failure analysis

### Local authentication

On Linux AF_UNIX transports, validate peer credentials with `SO_PEERCRED`, as specified by [`unix(7)`](https://man7.org/linux/man-pages/man7/unix.7.html). On supported Apple transports, use `getpeereid`, whose official [manual](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/getpeereid.3.html) reports the peer's effective UID and GID.

Both connector and provider must validate the expected current user. Put registry directories and sockets under user-only permissions, create registry records atomically, bound their count and age, and treat every path as an untrusted locator until peer authentication and handshake succeed.

Peer UID and a fresh challenge prove same-user reachability and freshness. They do not prove that the peer is the intended provider. A provider extension needs an authenticated bootstrap, preferably a single-use capability inherited from a provider-owned launch or exchanged over an already authenticated provider control channel. The subsequent handshake binds that bootstrap to provider, protocol, durable session ID, live incarnation, endpoint owner, capabilities, and connection generation. A PID, path, registry filename, process nonce, self-asserted provider proof, or label by itself is never authority. Credentials remain in memory or an OS credential facility and never enter the binding preference, journal, logs, diagnostics, or support bundle.

An arbitrary malicious process already running with the user's privileges is outside the fully defendable boundary. It may be able to inspect the user's files or process state and steal bootstraps. Same-user peer checks still prevent cross-user and accidental endpoint confusion, but they cannot justify a claim of resistance to complete current-user compromise. Providers without an authenticated bootstrap may be used only under an explicit same-user trust assumption, and the residual impersonation risk remains high.

### Cooperating lease semantics

One cooperating Proqi connection per harness session is a current-user coordination rule. The provider must authoritatively grant, renew, and revoke the lease when exclusive Proqi binding matters. A local lock only coordinates Proqi instances.

Kubernetes client-go's [leader-election implementation](https://github.com/kubernetes/client-go/blob/83efbe2ff83ef990af5bf87388c51ad5362a7bf5/tools/leaderelection/leaderelection.go) explicitly warns that its lease does not provide fencing. The same limit applies here: a timed lease cannot prove that a harness UI, SDK, old process, or malicious local client is excluded. Product copy must say `linked by another Proqi session`, not `agent exclusively controlled`.

### Receipt and dedupe precedent

NATS JetStream distinguishes a persisted publish acknowledgement from later consumption and uses a caller-supplied message ID for duplicate suppression within a configured window. See the primary [JetStream concepts](https://github.com/nats-io/nats.docs/blob/master/nats-concepts/jetstream/README.md) and [development guide](https://github.com/nats-io/nats.docs/blob/master/using-nats/jetstream/develop_jetstream.md). The relevant lesson is not to copy NATS. It is to require an explicit durable acknowledgement and dedupe scope before claiming durable queueing or retry safety.

### Capability negotiation precedent

The Language Server Protocol exchanges client and server capabilities during initialization and treats them as part of the connection contract. See the primary [LSP 3.17 specification](https://github.com/Microsoft/language-server-protocol/blob/2e5d8b6f223371b6a2d3f39a640488f895dbb060/_specifications/lsp/3.17/specification.md). Proqi likewise needs an explicit protocol family, version range, capability set, and required delivery mode. Unknown mandatory capabilities and unsafe downgrade produce `Incompatible`, not best-effort behavior.

### Stable identity and ephemeral connection precedent

OpenSSH separates durable host identity and connection preference from each transient transport. The pinned portable source at [`7fe3b24c922b7af2d743737f7cf37df61ea06426`](https://github.com/openssh/openssh-portable/tree/7fe3b24c922b7af2d743737f7cf37df61ea06426) reads durable configuration in [`ssh_config.5`](https://github.com/openssh/openssh-portable/blob/7fe3b24c922b7af2d743737f7cf37df61ea06426/ssh_config.5), verifies the freshly presented host key in [`sshconnect.c`](https://github.com/openssh/openssh-portable/blob/7fe3b24c922b7af2d743737f7cf37df61ea06426/sshconnect.c), and negotiates fresh transport state described in [`PROTOCOL`](https://github.com/openssh/openssh-portable/blob/7fe3b24c922b7af2d743737f7cf37df61ea06426/PROTOCOL). A changed host identity is surfaced rather than repaired from a friendly label.

This is a design precedent, not a drop-in authority. Proqi's separation of durable session address from live incarnation and route is an inference from OpenSSH identity checking, Kubernetes lease limits, LSP negotiation, NATS receipts, Sendoff interaction behavior, and Proqi's own recovery invariants. No single studied system covers the complete persistent agent-binding lifecycle.

### Ranked risks

Detectability means how readily the defect is noticed before harm.

| Risk | Severity | Likelihood | Detectability | Mitigation | Residual |
| --- | --- | --- | --- | --- | --- |
| Wrong live target through label collision or reused route | Critical | Medium | Low | Exact durable ID, peer proof, incarnation, generation, pre-send revalidation; labels presentation only | Low |
| Accidental or cross-user endpoint substitution | Critical | Medium | Low | User-only registry, peer credentials, authenticated bootstrap, bounded age, atomic records | Low |
| Request or receipt replay across live generations | Critical | Medium | Low | Bind operation, submission ID, digest, session, incarnation, and generation into the signed or authenticated proof | Low after provider qualification |
| Duplicate after lost receipt | Critical | Medium | Low | Durable provider dedupe, query by submission ID, no automatic retry | Medium until providers qualify |
| Terminal bytes misreported as accepted | Critical | Medium | Medium | Typed assurance; terminal bytes never remove sources | Low |
| False exclusivity from a local lease | High | Medium | Low | Provider-authoritative lease and truthful copy; explicit takeover | Medium |
| Old status overwrites a new binding | High | Medium | Medium | Binding fingerprint and generation on every async result | Low |
| Mixed protocol or capability downgrade | High | Medium | High | Negotiation, required capabilities, version refusal | Low |
| Provider update changes semantics | High | Medium | Medium | Compatibility fixtures, allowlisted versions, doctor, fail closed | Medium |
| Provider crash after acceptance | High | Medium | Low | Durable dedupe ledger and receipt query; unknown keeps source | Medium |
| Slow reader, oversized frame, or queue exhaustion | High | Low | High | Strict frame, prompt, queue, deadline, and concurrency bounds | Low |
| Malicious same-user client or provider impersonation | Critical | Low | Low | Authenticated bootstrap where available; explicit same-user trust boundary; no bearer material in registry | High under current-user compromise |
| Credential or path leakage in support output | High | Low | High | Enumerated redacted diagnostics and snapshot tests | Low |
| Partial SQLite migration or mixed writer | Critical | Low | High | Exclusive lease, backup, transaction, integrity check, protocol bump | Low |
| Board binding confused with attached-view adjacency | Critical | Medium | Medium | Distinct typed addresses and owner scopes | Low |
| Discovery failure presented as no targets | Medium | High | High | Per-provider completeness and reason codes | Low |
| Revoked provider permission | High | Medium | High | Classified disconnect, fresh handshake, no credential fallback | Low |
| Suspension or network-like socket loss | Medium | High | High | Freshness expiry, bounded reconnect, no resend | Low |

All messages, frames, prompts, catalogs, registries, queues, retries, and teardown need explicit bounds. A provider that cannot state its maximum accepted prompt and frame size is incompatible until an adapter supplies a safe lower bound.

## Exact user and system sequences

### Link

1. User invokes Link to agent.
2. Application opens the shared catalog in Link mode and starts independent bounded discovery.
3. UI shows entries and truthful provider completeness.
4. User highlights one exact established session. Navigation and a row click change only this local selection.
5. User explicitly activates the labelled Link action.
6. Application resolves the durable address again, authenticates the peer, negotiates protocol and capabilities, proves incarnation, and requests the cooperating bind.
7. Application persists a content-redacted lifecycle operation before any binding mutation.
8. A native provider grants an idempotent short-lived provisional lease; Herdr acquires only the local cooperating Proqi lease.
9. One SQLite transaction commits the preference and local lifecycle phase.
10. A native provider finalizes with the same operation ID. A lost result becomes binding-operation unknown and is queried after fresh authentication.
11. Status projects the current label and live state. No thought, focus, selection, or provider prompt changes.

### Reconnect

1. Status shows disconnected or reconnect available for the persisted exact address.
2. User invokes Reconnect, unless all silent transport-refresh conditions hold.
3. Application rediscovers only that address, not a similar label or last target.
4. A new incarnation is named and confirmed.
5. Fresh peer, protocol, capabilities, binding ownership, and liveness proofs create a new generation.
6. Failure leaves the preference visible and content untouched.

### Disconnect

1. User invokes Disconnect for now, or a generation-matched transport loss starts the same local transition.
2. Application records a lifecycle operation, cancels admission for the current generation, and sends one idempotent bounded release when the provider supports it.
3. The live route is invalidated regardless of release response. The durable preference remains.
4. A confirmed release shows disconnected with Reconnect available. A lost release response shows connection operation unknown and disables delivery until an authoritative query or lease expiry.
5. Disconnect never clears the preference, submits content, or retries a delivery.

### Send and keep

1. User invokes Submit to linked agent and keep.
2. Existing application owners capture sources, revisions, exact assembly, digest, and attachment health.
3. Store persists `prepared` and locks sources, then persists `sending`.
4. Connection manager revalidates durable ID, live incarnation, endpoint owner, protocol, capabilities, lease, and generation.
5. Delivery sends one request containing the stable submission ID.
6. A matching qualifying receipt transitions the attempt to accepted.
7. Sources remain by explicit keep policy. Locks release through the terminal journal state.

### Send and remove

1. Application checks that the bound route advertises a qualifying removal assurance. A global or linked protocol 19 Herdr route fails here, sends nothing, retains sources, and offers the explicit keep action.
2. Steps 1 through 6 of Send and keep then run with remove policy for a qualifying route.
3. `TerminalBytesQueued`, rejected, mismatched, stale, incompatible, and unknown outcomes retain every source.
4. On a qualifying accepted or durably queued receipt, one transaction records the receipt and removes only source revisions that are unchanged.
5. Changed sources remain.
6. The Board operation makes accepted removal undoable and redoable after restart.

### Stale binding

1. Pre-send validation cannot resolve the exact durable session ID.
2. Application performs no delivery and writes a terminal failed or stale classification according to whether an attempt already entered the journal.
3. Sources remain locked only until the local terminal state commits.
4. Status becomes stale and offers Change or Unlink. Reconnect is offered only if the same durable identity is discoverable.
5. No label, last target, adjacent pane, or raw terminal path is tried.

### Target replacement

1. A registry or pane route now points to a different session or incarnation.
2. Peer and identity proof conflict with the current live connection generation.
3. Application invalidates the route before sending and discards late status events from it.
4. Same label and project context do not repair the mismatch.
5. The user explicitly reconnects the same durable session if it exists, or changes the binding.

### Outcome unknown

1. The provider may have received the request, but Proqi lacks a qualifying receipt.
2. Attempt becomes outcome unknown and sources remain.
3. The status row and recovery details name the exact affected attempt without showing content.
4. Proqi queries a provider ledger only when the negotiated capability guarantees durable submission-ID lookup.
5. Otherwise the user chooses whether to resend, with an explicit duplicate warning.

### Takeover

1. Link or reconnect receives an authoritative `held by another cooperating Proqi connection` result.
2. UI names the board and safe owner context when available and asks for explicit takeover.
3. Application journals a unique takeover operation before the provider revokes or transfers the old lease.
4. A confirmed result returns a new binding proof and generation.
5. An authoritative refusal leaves content and preference unchanged. A lost response becomes binding-operation unknown, disables delivery, and is queried by operation ID. It is never blindly retried.
6. No claim is made about excluding the provider UI or unrelated clients.

### Consolidated failure matrix

| Event | Preference | Live connection | Sources and journal | Status | Safe next action |
| --- | --- | --- | --- | --- | --- |
| Provider crash before delivery acceptance | Keep | Invalidate | Rejected if authoritative, otherwise outcome unknown; keep sources | Disconnected plus attempt result | Reconnect; resend only by explicit user choice |
| Provider crash after possible acceptance | Keep | Invalidate | Outcome unknown; keep sources | Delivery outcome unknown | Query durable provider ledger if qualified |
| Proqi crash during delivery | Keep | Rebuild from nothing | `prepared` cancels; `sending` becomes outcome unknown | Recovery detail on restart | No automatic resend |
| Proqi crash during Link | Recover from lifecycle row | Never reuse old proof | No content involved; abort provisional or query operation | Connecting, unknown, or unlinked | Fresh query; provisional expiry |
| Proqi crash during Change | Old or new address according to atomic local commit | Invalidate both generations | No content involved; old route cannot send after swap | Change pending or operation unknown | Query both operation IDs; release orphan by ID |
| Proqi crash during Unlink | Preference cleared with durable tombstone | Invalidate | No content involved | Unlink pending or unlinked | Query release or await expiry |
| Socket loss | Keep | Invalidate generation | In-flight send is unknown; otherwise no attempt | Disconnected or delivery unknown | Silent refresh only under exact safe conditions |
| Operating-system suspension | Keep | Liveness expires | In-flight send is unknown after deadline | Status unavailable or disconnected | Fresh validation after resume |
| Harness session switch | Keep old address | Invalidate | Pre-send fails; keep sources | Stale | Reconnect exact old ID, Change, or Unlink |
| Endpoint replacement | Keep | Invalidate | Pre-send fails or in-flight becomes unknown | Reconnect available only after exact proof | Fresh handshake; no request replay |
| Permission revoked | Keep | Invalidate | Rejected or pre-send failure; keep sources | Disconnected, permission denied | Restore permission, Change, or Unlink |
| Provider status refresh fails while socket stays open | Keep | Route may remain, freshness does not | New delivery disabled until pre-send proof | Status unavailable, not idle | Refresh or Reconnect |
| Update replaces Proqi owner | Keep | Old generation cancelled | Owner handoff preserves journals; in-flight send remains conservative | Reconnecting or recovery detail | New owner validates from scratch |
| Mixed-version replacement | Keep | No unsafe attach | No new write; legacy attempts unchanged | Incompatible | Finish update or use compatible owner |
| Takeover response lost | Keep last committed address | Invalidate | Lifecycle operation unknown; no prompt involved | Ownership unknown | Authoritative query or lease expiry, never blind retry |
| Partial SQLite persistence | Last committed transaction | Invalidate on restart | Integrity and migration recovery; never infer remote result | Recovery required | Restore backup or reconcile operation journal |

## Implementation stages

Estimates are engineering ranges, not commitments. Each stage has an independent acceptance boundary.

| Stage | Estimate | Entry criterion | Acceptance criterion | Rollback or disable path |
| --- | --- | --- | --- | --- |
| 0. Behavior-neutral adjacent split | 3 to 5 days | Current adjacent tests green | Same CLI, JSON, storage, behavior, errors, and snapshots | Revert refactor; no durable change |
| 1. Managed global Herdr catalog, route migration, and Send once | 9 to 14 days | Stage 0 landed; terminal-assurance decision ratified | Schema and storage protocol decode legacy adjacent rows, encode global routes, make new global routes keep-only, apply the separately ratified adjacent policy, and ship exact managed discovery with truthful completeness | Disable global commands; forward journal data stays readable; adjacent route remains available |
| 2. Herdr namespace, standalone locator, preference migration, Link, and status | 10 to 15 days after Herdr prerequisites | Stable server namespace, authenticated standalone locator, and established session IDs proven | Binding preference and lifecycle-operation migrations precede Link; standalone restart, partial-operation, stale, replacement, status, change, disconnect, unlink, and reconnect tests pass | Disable Link UI; forward preference and lifecycle rows remain inert and readable |
| 3. Native provider-neutral foundation | 8 to 12 days | Stage 2 usability evidence accepted | Native address discriminant, connector, lifecycle, capability, assurance, and protocol fields migrate before a native consumer | Feature capability off; forward data remains preserved |
| 4. Pi extension research | 2 to 4 days for protocol proof; 2 to 3 weeks if qualified | Disposable Pi extension approved | Exact identity, peer proof, incarnation, modes, receipt, dedupe, status, crash matrix | Extension disabled or removed; no terminal fallback |
| 5. Additional provider adapters | 2 to 5 weeks each | Provider-specific qualification gates pass | Ordinary live session controlled with equal assurance | Per-provider disable, preference reports unavailable |

Stages 1 and 2 can ship useful keep-only Herdr behavior without claiming native-provider support. Every migration now lands in the same stage before its first consumer. Stage 3 should not add its native discriminant or protocol fields until the Stage 2 product model is accepted. Provider adapters are independently gated and removable.

## Compatibility and migration plan

| First consumer | Durable change that must land first | Backward behavior | Mixed-version behavior |
| --- | --- | --- | --- |
| Global Herdr Send once | Submission address kind and fingerprint version with `AdjacentHerdrV1` and `GlobalHerdrV1` | Existing rows decode exactly as legacy adjacent attempts; historical accepted rows are not rewritten | Older owners refuse the newer storage protocol |
| Herdr Link | Per-session binding preference plus lifecycle-operation journal | Sessions without a row are unlinked; removing UI leaves rows inert and readable | Only the new owner may mutate binding rows; attached old views receive incompatible capability |
| Native binding | `NativeV1` address, protocol family, capability and assurance fields | Herdr rows retain their exact discriminant and fingerprint | Provider is unavailable to older clients; no lossy down-conversion |
| Stronger Herdr receipt | New negotiated capability and assurance spelling | Protocol 19 remains terminal admission and keep-only | Unknown assurance fails closed |

Each SQLite migration is forward-only, lease protected, backed up, transactional, and followed by integrity checks. Fingerprint algorithms are versioned. A migration never recomputes old fingerprints with new fields. Any `prepared` or `sending` legacy attempt is recovered under its original state machine before the new owner accepts submissions.

The control protocol, CLI JSON, diagnostics schema, stable error vocabulary, current-contract fixtures, and prepared GitHub Release notes change in the same implementation commit when their serialized shape changes. There is no automatic downgrade. Disabling a provider or UI leaves durable data intact so a compatible future owner can explain it.

## Required implementation verification

### Adversarial qualification matrix

| Scenario | Layer or provider | Setup | Oracle | Durable state | UI state | Ship gate |
| --- | --- | --- | --- | --- | --- | --- |
| Exact Herdr link after restart | Herdr and application | Stable server namespace, authenticated standalone locator, established session, new Proqi process | Same namespace and full session ID resolve; no pane-label repair | Preference unchanged; new live generation only in memory | Connecting, then idle or working | Required for Stage 2 |
| Protocol 19 adjacent remove request | Herdr assurance | Lower-case adjacent action after an approved assurance migration | Text is admitted once; no source is removed | `TerminalBytesQueued`; source revisions intact | `Terminal input queued; source kept` | Required only if Oliver approves the adjacent behavior change |
| Protocol 19 global or linked remove | Herdr assurance | Explicit remove action against keep-only target | Capability preflight sends nothing | No attempt or source mutation | Remove unavailable; offer keep | Required before global delivery and Link |
| Strong native accepted receipt | Native provider | Exact ID, incarnation, mode, submission ID and digest | One attributable semantic acceptance | Accepted attempt; unchanged sources removable atomically | Accepted, then calm live state | Required for native remove |
| Durable queued receipt | Native provider | Provider crash after acknowledged queue write | Lookup by submission ID returns same receipt after restart | `ProviderQueuedDurably`; one provider record | Queued, not completed | Required if queue mode ships |
| Duplicate replay | Native provider | Repeat same ID and digest; then same ID with changed digest | First returns original receipt; second rejects | One provider turn and one dedupe row | No duplicate success | Required for native remove and recovery lookup |
| Same-incarnation transport refresh | Connector | Drop socket without sending request | Fresh peer and unchanged incarnation reconnect silently | Preference unchanged; no lifecycle mutation needed | Brief connecting, then prior fresh state | Required for auto refresh |
| New-incarnation resume | Connector | Same durable ID, new process generation | No automatic semantic reconnect | Preference kept; old generation invalid | Reconnect available with confirmation | Required for Stage 2 |
| Label collision and route reuse | Catalog and connector | Duplicate labels, moved panes, reused PID, endpoint, or route | Only exact namespace, session ID, incarnation, and peer proof match | No preference repair | Stale or explicit choices | Required for every provider |
| Link crash boundaries | Lifecycle journal | Crash after each prepared, provisional, local commit, and finalize boundary | Query or expiry reaches one explainable owner state | Journal reaches complete, cancelled, or unknown without secrets | Connecting, unlinked, connected, or ownership unknown | Required before persistence ships |
| Change and Unlink crash boundaries | Lifecycle journal | Crash at every old-release, new-provisional, swap, and finalize boundary | At most one locally routable preference; orphan leases expire or release by operation ID | Atomic preference or tombstone plus recoverable operation | Change pending, unlink pending, or clear result | Required before actions ship |
| Takeover lost response | Provider lease | Drop response after possible revocation | No blind retry and no delivery until authoritative query | Binding operation unknown | Ownership unknown | Required where takeover is advertised |
| Stale status result | Status worker | Delay old generation response past Change or Unlink | Result is discarded | No durable mutation | New binding state remains visible | Required for Stage 2 |
| Provider discovery partial failure | Catalog | One timeout, one incompatible source, one healthy source | Healthy entries remain and completeness is false | No durable mutation | Section reasons plus healthy choices | Required for shared chooser |
| Shared-session attached view | Control protocol | Two views, board binding, different local adjacent panes | Board owner routes explicit bound action; each view retains local adjacency | One board preference | Distinct linked and adjacent projections | Required before shared-session coexistence |
| Oversized or slow peer | Adapter | Maximum-minus-one, maximum, oversized, partial, and slow frames | Bounds and deadlines hold; no content leaks | Rejected or unknown terminal state as appropriate | Bounded classified failure | Required for every socket provider |
| Mixed-version owner replacement | Storage and control | Old attached view, new owner, schema migration | Old writer refuses; new owner preserves legacy attempts | Exact forward migration | Update required or compatible read-only view | Required with each protocol bump |

### Domain and application

- Constructors reject empty, oversized, malformed, and cross-provider IDs.
- Every state transition is exhaustive and invalid transitions fail without mutation.
- Old-generation connect, status, receipt, and teardown events are ignored.
- Capability downgrade and unknown mandatory capability fail closed.
- Link selection never captures, locks, focuses, or submits thought content.
- One-off send never changes binding preference.
- Change is atomic after successful bind.
- Unlink remains durable when remote release times out.

### Delivery and recovery

- Exact single and multi-thought bytes remain unchanged.
- Shared command-starter assembly remains unchanged.
- Attachment preflight fails before external delivery.
- Multi-selection, submit all, keep, remove, changed-source retention, locks, and receipt mismatch use the existing owners.
- Crash is exercised before acceptance, after possible acceptance, after receipt before local commit, and during local removal.
- Unknown outcomes never auto-retry.
- Duplicate submission IDs return one provider receipt or qualify the provider as non-idempotent.
- Terminal byte injection can never construct an assurance that permits removal.

### Discovery and identity

- Provider sources fail independently and global completeness remains truthful.
- Duplicate labels, same directory, moved pane, reused endpoint, reused PID, new incarnation, fork, delete, and session switch are adversarial fixtures.
- Provisional Herdr sessions cannot be linked.
- Exact established Herdr sessions can move panes without label repair.
- Endpoint substitution, stale registry replay, wrong UID, and wrong challenge fail before content leaves Proqi.

### UI and accessibility

- Link and Send chooser modes have distinct labels and actions.
- Footer states cover every vocabulary item in wide, narrow, tall, and shallow snapshots.
- Actionable failure remains legible without color.
- Commands, Help, shortcuts, labels, render geometry, and pointer hit mapping share canonical definitions.
- Keyboard and mouse paths are behaviorally equivalent.
- Long duplicate names, wide characters, combining marks, limited color, and terminal default palettes remain correct.

### Storage and protocol

- Forward migration preserves every legacy adjacent attempt and its fingerprint version.
- Mixed owner versions refuse unsafe writes.
- Migration interruption restores or resumes from a valid backup and passes integrity checks.
- Preferences never serialize endpoints, tokens, PIDs, nonces, generations, leases, or liveness.
- Diagnostics snapshots prove content and path redaction.
- Control protocol tests distinguish board-wide binding from attached-view adjacent context.

## Evidence appendix

### Repository evidence

The following contracts were read completely before the design was written: [`context/PRODUCT.md`](../context/PRODUCT.md), [`context/ARCHITECTURE.md`](../context/ARCHITECTURE.md), [`context/TODO.md`](../context/TODO.md), and [`context/FEATURE_INVENTORY.md`](../context/FEATURE_INVENTORY.md). The current primary checkout's complete `context/TODO.md` and its uncommitted diff were read as product context only. The primary `.gitignore`, roadmap, and worktree were not modified.

Implementation evidence included the agent port and Herdr adapter, UI preparation and delivery owners, target identity, store port, SQLite submission journal and schema, integration context, active-owner control, update coordination, status composition, Commands, cross-session transfer, invocation discovery, migration tests, Herdr executable tests, UI submission tests, SQLite recovery tests, and PTY lifecycle tests. Repository history, all open issues, all open pull requests, and active worktrees were inspected read-only.

At research time, open [issue 52](https://github.com/oborchers/proqi/issues/52) concerned a blocked terminal input reader, open [issue 56](https://github.com/oborchers/proqi/issues/56) concerned Home and End behavior, and draft [PR 48](https://github.com/oborchers/proqi/pull/48) implemented smart paste reflow. No active agent-binding implementation was found.

### External primary evidence

- Sendoff exact code, history, docs, and tests at `e90af467be9541796af99600b1d484a8e5e82172`, with specific links in the Sendoff section.
- Pi public source at `17de82d7bea18a6589677a9761baabc2060c9efb` and the repository's earlier 0.84.3 spike evidence.
- Hermes public source at `b0ab2e163a50d4e6c36507eba955a6067fde6abc` and the repository's earlier 0.20.6 spike evidence.
- Codex public source at `9c4253ffc1b954337bf2f494aadc55e9cd132a48`, local CLI 0.153.2, and the repository's earlier 0.150.1 spike evidence.
- Claude Code official Channels, Channels reference, plugin, MCP, and session documentation, local CLI 2.1.260, and the repository's earlier 2.1.251 spike evidence.
- OpenSSH portable at `7fe3b24c922b7af2d743737f7cf37df61ea06426`, Kubernetes client-go at `83efbe2ff83ef990af5bf87388c51ad5362a7bf5`, LSP at `2e5d8b6f223371b6a2d3f39a640488f895dbb060`, NATS JetStream primary documentation, Linux `unix(7)`, and Apple's `getpeereid(3)` manual.

The design conclusions that combine these systems are explicitly inferences. No cited external system is treated as authority for Proqi's complete lifecycle.

### Current Proqi experiment oracles and results

Before running each current-Proqi experiment, the oracle was defined as follows: exact bytes reach only the revalidated target; target replacement sends nothing; failed and mismatched attempts retain sources; accepted removal is durable and undoable; an in-flight crash becomes outcome unknown without removing sources; a real PTY keeps an unroutable draft; owner restart restores control when the endpoint becomes ready.

The following focused tests passed at the studied Proqi revision:

```text
cargo test --lib submission_revalidates_and_passes_exact_text_as_one_distinct_argument
cargo test --lib opencode_replacement_before_delivery_sends_nothing
cargo test --test herdr_executable recorded_fake_executable_proves_direct_semantic_cli_contract
cargo test --test ui_board failed_submission_preserves_thought_and_accepted_remove_is_undoable
cargo test --test ui_board accepted_receipt_rejects_a_different_stable_target
cargo test --test ui_board direct_submit_removal_waits_for_durability_and_retries_without_losing_edit_state
cargo test --test sqlite_store restart_marks_a_multi_source_send_unknown_without_removing_sources
cargo test --test pty primary_enter_variants_reach_edit_submission_and_retain_an_unroutable_draft
```

Two sandboxed focused attempts at

```text
cargo test --test pty active_tui_accepts_durable_idempotent_cli_mutations_before_crash
```

failed after about 16.5 seconds at `tests/pty/support.rs:37` with `owner did not advertise a ready control endpoint`. The canonical gate initially confirmed the environmental cause when nine control tests failed to bind Unix sockets with `Operation not permitted`. The unchanged gate was then run with normal isolated socket permissions. All 1,365 tests passed, including the owner-restart PTY test, and the one documentation test passed. Owner restart was therefore reverified; the sandbox-only failures were not product failures.

No prompt was sent into a real user agent. No private session, external installation, provider configuration, or primary checkout was modified. Pi and Hermes were absent locally. Codex, Claude Code, and Herdr were inspected read-only. A successful command exit was never treated as semantic acceptance.

### Negative and untested evidence

- Sendoff dependency installation was blocked by sandbox temporary-directory permissions, and escalation for package installation scripts was rejected. Its pure resolution tests passed 15 of 15. Persistence and concrete delivery adapters were inspected statically.
- No provider-native endpoint was launched. Pi, Hermes, Codex, and Claude Code production viability therefore rests on current primary contracts plus the repository's completed isolated spikes, not a new live send.
- Herdr protocol 19 exposes no stable server namespace. Persistent Herdr Link remains conditional on that prerequisite.
- No current provider proved durable submission dedupe for this design.
- Same-user peer credentials do not defeat a malicious process with complete access to the user's privilege domain.
- Provider completion and blocked-status attribution were not newly exercised.

### Quality gates

`cargo xtask check` passed with 1,365 tests, 5 suite-defined skips, and 1 documentation test after rerunning with normal Unix-socket permissions. The first restricted run failed because the sandbox returned `Operation not permitted` for local control socket binds. `git diff --check` passed.

`cargo xtask audit` and `cargo xtask package` were not run. This is a documentation-only research PR, not a release or product implementation milestone, so the canonical local check is the proportionate gate. No package or artifact was produced.

## Roadmap amendment recommendation

Do not copy or replace the user's current roadmap wholesale. Amend its sequence narrowly after this decision is accepted:

1. Make the behavior-neutral adjacent split a hard prerequisite.
2. Put the global Herdr route migration, catalog, and keep-only one-off send before persistent Link.
3. Make a stable Herdr server namespace a prerequisite for persistent Link, then land the preference and lifecycle-operation migrations before their first consumer.
4. Classify every protocol 19 route as terminal admission. Preserve historical records. Subject to Oliver's explicit product decision, let new lower-case adjacent outcomes retain their source unless Herdr gains stronger assurance.
5. Add native discriminants and protocol fields only after the Herdr product validation.
6. Make attributable acceptance, live incarnation, authenticated bootstrap, and durable provider idempotency named gates for native remove-after-success.
7. Keep Pi as the first native extension candidate.
8. Update Claude Code from unavailable upstream to a research-preview Channels candidate, with the documented constraints preserved.
9. Keep Global Herdr delivery and Shared Proqi sessions separate, sharing only board-owner and control-protocol prerequisites.

## Final recommendation

Proceed with the invariant and Herdr-first keep-only validation after Herdr exposes a stable server namespace and authenticated standalone locator. Defer generic native-provider implementation until the interaction model is proven and at least one provider extension meets the identity, authenticated-bootstrap, incarnation, receipt, and idempotency gates. Reject terminal injection, terminal-admission removal for new global or persistent routes, label repair, last-target fallback, and command-success-equals-accepted semantics. Treat adjacent removal as the separate explicit product decision recorded above.
