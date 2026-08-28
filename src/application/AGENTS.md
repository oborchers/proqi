# Proqi application-layer contract

The repository root contract applies here. This scope owns use-case policy,
state transitions, application services, and terminal-independent effects.

## Ownership

- Application code depends only on domain values and ports. It never imports
  `cli`, `ui`, concrete adapters, Crossterm, Ratatui, SQL, filesystem APIs, or
  process syntax.
- A rule shared by the TUI, CLI, owner-control protocol, recovery, or external
  edits belongs in an application service or named application policy. Do not
  reproduce use-case rules in callers.
- Outbound prompt assembly belongs to `application::prompt`. Stored thoughts
  remain canonical and unchanged; harness-specific assembly policy applies only
  at the outbound boundary.
- Finite actions, effects, operation kinds, replay kinds, and failure classes
  stay typed. Consumers route them exhaustively so adding a variant cannot
  silently become a no-op.

## Durable mutations and restored state

- Use the canonical service helpers for loading live state and committing one
  application effect. Do not build a parallel load/commit path for one caller.
- Replay and idempotency matching use the canonical operation matcher across
  structural changes, history, collapse, and external edits. A request either
  matches the complete typed operation or fails explicitly.
- Validate every session snapshot restored from a port before it enters live
  application state. Constructors and rehydration reject impossible ordering,
  identifiers, cursor state, annotations, and operation sequences.
- Preserve optimistic state and durability acknowledgements as separate facts.
  Storage failure must remain typed, must not discard recoverable in-memory
  content, and must never be reported as durable success.
- One user intention produces one atomic application transaction and one undo
  unit unless the product contract explicitly defines otherwise.

## Tests

- Reducer tests prove state and requested effects; service tests prove port
  orchestration, replay, idempotency, restored-state validation, and failure
  classification without concrete adapters.
- Keep behavior-owned unit tests adjacent. Use top-level integration or PTY tests
  only when the contract crosses process, adapter, or terminal boundaries.
