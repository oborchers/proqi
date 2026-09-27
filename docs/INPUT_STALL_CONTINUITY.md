# Input Stall Continuity Research

Status: implementation record for bounded exact-session recovery

Last reviewed: 2026-09-14

## Problem boundary

Proqi already detects a confirmed Crossterm input-reader stall from a separately
scheduled supervisor, protects durable state, restores the terminal, and exits
with an exact resume command. The continuity fallback changes only that
confirmed active-session failure. It performs one safe same-pane Unix `exec`
replacement after the ordinary quiescence boundary, then requires fresh input
lane progress before the incident is considered recovered.

This is not an upstream input worker replacement. The nested reader may remain
detached until process replacement, and a revoked PTY remains unrecoverable.
[Proqi issue 52](https://github.com/oborchers/proqi/issues/52) stays open.

## Adopted policy

1. The state machine is `Healthy`, `ConfirmedStall`, `RecoveryPrepared`,
   `Probation`, then `Healthy`.
2. One automatic `exec` is allowed per incident. A probation stall never
   restarts again.
3. Three completed bounded polls from the canonical input source prove probation
   health. Startup and elapsed time do not.
4. Each exact SessionId may start at most two automatic recoveries in a rolling
   ten-minute window. Older attempts expire before the next admission decision.
5. The content-free runtime record is limited to 16 KiB, stored with current-user
   ownership and private permissions, and bound to session, PID, executable
   digest, lineage, attempt, and previous instance.
6. Recovery reuses the terminal runner's normal two-second shutdown deadline and
   the exact Unix replacement owner introduced for update convergence. It adds
   no storage schema or protocol.
7. Board, Compose, and Edit restore a validated content-free interaction
   checkpoint. Modal state and the sessionless Browser owner fail closed.
8. If an ordinary healthy launch cannot establish a private recovery record or
   read the current executable identity, the session still starts but automatic
   recovery is disabled for that process. An exec replacement launch remains
   strict and fails closed on the same ambiguity.
9. Session and schema leases remain held until every owned worker has stopped.
   Cleanup results are validated while those leases remain held. They are
   released only before the recovery record is prepared and `exec` is attempted.
10. A persistence failure prevents `exec`. Recovery shutdown admits and drains
    one existing private optimistic-state export. If its primary directory is
    unavailable, it makes one bounded attempt under the private runtime root,
    then reports the successful path beside the exact durable-session resume
    command. A successful retained-write retry returns the session to the
    ordinary replacement path.

## Upstream and open-source evidence

1. [Crossterm issue 1110](https://github.com/crossterm-rs/crossterm/issues/1110)
   records current Unix TTY error loops and the distinction between a dead TTY
   and a viable descriptor whose consumer has stopped progressing. Crossterm is
   MIT licensed. It was used as behavioral evidence only.
2. [Crossterm pull request 1067](https://github.com/crossterm-rs/crossterm/pull/1067)
   reports EOF and non-retryable Unix event-source failures instead of retrying
   them indefinitely. The discussion explicitly leaves consumer reopen policy
   to the consumer. Crossterm is MIT licensed. No source was copied.
3. [Crossterm pull request 1070](https://github.com/crossterm-rs/crossterm/pull/1070)
   explores delivering the same terminal errors through `EventStream`. Crossterm
   is MIT licensed. Proqi keeps its synchronous input owner and implements its
   continuity policy independently.
4. [iocraft pull request 218](https://github.com/ccbrown/iocraft/pull/218)
   propagates terminal input errors through another Crossterm consumer. iocraft
   is MIT licensed. It corroborates failure propagation only, and no source was
   copied.
5. [OpenAI Codex EventBroker](https://github.com/openai/codex/blob/main/codex-rs/tui/src/event_broker.rs)
   owns a recreatable Crossterm stream and keeps reopen policy in the consumer.
   Codex is Apache-2.0 licensed. Proqi does not copy this implementation, adopt
   its async runtime, or treat stream recreation as proof of health.

## Rejected alternatives

Restarting a reader thread in place cannot reclaim the current blocking
Crossterm call or prove that its global event-reader ownership was released.
Starting another reader risks duplicate terminal consumption and unbounded
detached threads. Replacing the process before quiescence risks lost optimistic
edits, duplicate external effects, and stale runtime leases. Treating successful
startup as health creates an `exec` loop on a revoked or persistently broken
PTY. A process-lifetime one-shot flag prevents independent recoveries hours
later, while a global budget lets one session exhaust another.

The selected policy therefore replaces the exact quiesced process, requires
canonical input progress, scopes the circuit to one exact session, and fails
closed at every ambiguous boundary.
