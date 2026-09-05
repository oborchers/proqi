# Input Watchdog Suspension Research

Status: implementation record for the input heartbeat lease

Last reviewed: 2026-09-05

## Problem boundary

The terminal input supervisor observes messages from a nested Crossterm reader.
Silence is evidence that the reader is unhealthy only while the supervisor is
itself being scheduled. A process-wide pause can make both timestamps old, so
the first supervisor turn after the pause cannot attribute the silence to the
reader alone.

This policy does not solve the detached reader tracked by
[Proqi issue 52](https://github.com/oborchers/proqi/issues/52). Crossterm pull
request 1067 remains open and unreleased. The nested reader and its existing
bounded join behavior remain unchanged.

## Clock semantics

Rust documents `Instant` as monotonic but deliberately does not promise whether
system suspend counts as elapsed time. The current implementation table uses
`CLOCK_UPTIME_RAW` on Darwin and `CLOCK_MONOTONIC` on other Unix targets. Apple
documents that its uptime clock does not advance while the system sleeps. The
Linux manual documents the same behavior for `CLOCK_MONOTONIC`, while
`CLOCK_BOOTTIME` is the separate suspend-inclusive clock.

Consequently, ordinary full system sleep does not consume this lease on the
currently supported platforms. Display-only sleep, App Nap, `SIGSTOP`, virtual
machine suspension, and severe scheduler delay can still leave the process
unscheduled while the monotonic clock advances. Comparing two observations by
the same supervisor detects that condition without depending on wall-clock
time or platform notifications.

Primary references:

1. [Rust `Instant`](https://doc.rust-lang.org/stable/std/time/struct.Instant.html),
   Rust project, MIT or Apache-2.0. The documentation and platform table were
   used as behavioral references. No implementation was copied.
2. [Apple `mach_absolute_time`](https://developer.apple.com/documentation/driverkit/mach_absolute_time),
   Apple documentation. It establishes that the corresponding uptime clock
   excludes system sleep. No Apple code was used.
3. [Linux `clock_gettime`](https://man7.org/linux/man-pages/man2/clock_gettime.2.html),
   Linux man-pages documentation. It distinguishes suspend-excluding
   `CLOCK_MONOTONIC` from suspend-inclusive `CLOCK_BOOTTIME`. No implementation
   was copied.

## Scheduling policy

The supervisor records both the last reader response and its own last
observation. A timeout after normally spaced supervisor observations consumes
the existing 500 millisecond reader lease. One supervisor observation gap of at
least the same lease proves that the supervisor itself did not observe the
reader during that period. It resets the reader lease once and records only the
bounded, content-free gap duration. If the reader remains silent while the
supervisor runs normally, the fresh lease expires with the unchanged typed
`Unresponsive` failure.

Tokio's missed-tick `Delay` policy is the closest established conceptual model:
after delayed scheduling resumes, the next interval is based on the current
observation instead of replaying stale elapsed ticks. Tokio is MIT licensed.
Proqi uses no Tokio code or dependency.

Reference:

1. [Tokio `MissedTickBehavior`](https://docs.rs/tokio/latest/tokio/time/enum.MissedTickBehavior.html),
   Tokio project, MIT. Conceptual reference only.

## Rejected alternatives

Platform sleep notifications do not cover `SIGSTOP`, App Nap, scheduler
starvation, virtual machine pauses, or downstream backpressure. They would also
require AppKit notification integration on macOS and the system D-Bus logind
contract on Linux. Those APIs add platform and lifecycle owners without making
the classification more complete.

References:

1. [Apple workspace wake notification](https://developer.apple.com/documentation/appkit/nsworkspace/didwakenotification),
   Apple documentation, reference only.
2. [systemd logind `PrepareForSleep`](https://www.freedesktop.org/wiki/Software/systemd/logind/),
   systemd documentation, LGPL-2.1-or-later project, reference only.

Small Rust packages do not solve this exact two-observer distinction. The
maintained `zeitstempel` crate, MPL-2.0, exposes explicit suspend-inclusive and
suspend-excluding clocks but does not classify supervisor scheduling gaps. The
`vigil` crate, MIT or Apache-2.0, runs a separate watchdog thread and requires
declared lease extensions. Both add more machinery than the local policy, and
the latter conflicts with the prohibition on another watchdog thread.

References:

1. [`zeitstempel`](https://docs.rs/zeitstempel/latest/zeitstempel/), MPL-2.0.
2. [`vigil`](https://docs.rs/vigil/latest/vigil/), MIT or Apache-2.0.

## Edge-case conclusions

1. A reader-only hang still accumulates normally observed time and fails at the
   existing boundary.
2. Supervisor starvation and whole-process `SIGSTOP` grant one fresh lease
   because neither provides reader-only evidence.
3. Repeated pauses replace stale silence independently and cannot emit duplicate
   failures.
4. Reader messages received after a pause remain authoritative and renew the
   lease normally.
5. Source-channel and downstream backpressure can delay the supervisor, so the
   same rule avoids blaming the reader for another lane's delay.
6. Continuous input, resize coalescing, event ordering, bracketed paste, EOF,
   terminal revocation, I/O failure, cancellation, panic handling, joins, and
   terminal restoration do not pass through the new timeout decision.
7. A real reader hang coincident with a supervisor pause receives one fresh
   lease, then fails within 500 milliseconds of normally scheduled supervision.
