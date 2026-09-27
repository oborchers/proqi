# Lifecycle PTY investigation

## Scope

This spike investigated three PTY failures reported by a feature worktree:

- keyboard quit while a Screenshot Inbox commit is delayed;
- termination while a Screenshot Inbox commit fails persistently;
- automatic release highlights after an in-app restart.

This lane's original base was `0b016f767af747f013301aaf41bcd0bf1cf827ef`.
The reporting worktree was `c5329152ff6d1f8f83c1f3cad1bee0e462342153`,
whose merge base with this lane was the earlier `e679483` revision.

## Revision evidence

The first failing main run at `e679483e64352ab2e3e70e734ed7e0f956a57d3f`
passed all three lifecycle cases. Its only PTY failures were two archived
update-migration fixture rewrites. Base `0b016f7` fixed that independent version
coupling by reading the archived manifest version. The later main CI and release
candidate for `0b016f7` were green.

The feature diff did not change terminal input admission, Screenshot Inbox
persistence, update acknowledgement, shutdown, or the three PTY fixtures. It
did change footer and Screenshot Inbox state layout and rendering. This
investigation did not serially run the cases at that reporting revision, so it
does not exclude a feature-side rendering interaction. Each worktree had its
own Cargo target directory, which rules out a shared stale test executable. The
release preparation changed package metadata and reviewed release inputs. It
did not change these lifecycle owners.

## Failure chains

### Keyboard quit during a delayed capture commit

The driver enters Edit, holds an immediate SQLite writer lock, admits a capture,
sends `!`, and waits for the reducer's test-only acceptance receipt. Exit 87
means that receipt was not observed. Quit was never sent, so the failure did not
exercise shutdown, restoration, cleanup, or persistence assertions.

The exact case passed alone outside the harness sandbox. Inside the sandbox it
failed earlier with exit 93 because Screenshot Inbox could not publish capture
authority. That is a permissions failure and not product evidence. The reported
exit 87 occurred in the general nextest pool, where the case did not own the
process and terminal resources that the dedicated serial PTY gate gives it.
That configuration mismatch is a mitigation target, not proof that scheduling
caused the reported outcome.

### Termination during a persistently failed capture

This case first holds the same SQLite writer lock long enough for the capture
save to fail. It then requests the explicit Retry action, sends `!`, and waits
for the Edit acceptance receipt before delivering termination signals. Exit 87
therefore occurs before termination. It does not prove an unbounded shutdown or
failed terminal restoration.

The exact case passed alone outside the sandbox, including truthful status 1,
bounded exit, released capture and instance authority, unchanged durable
content, and exact-session resume. Its distinct risk is the retry workflow's
additional persistence failure and replay work before the same bounded receipt
probe. It was also scheduled without exclusive nextest ownership in the failed
qualification. That scheduling difference is not causal proof.

### Release highlight dismissal

The fixture waited for `what's new` bytes, sent Escape, then polled the private
update cache for two seconds. Exit 96 means the durable acknowledgement was not
observed. Screen bytes are not reducer readiness: terminal output can reach the
Expect reader before the render call returns and arms the protected overlay's
input boundary. Under pooled scheduling, Escape can therefore be rejected as
pre-presentation input, or the accepted update effect can miss the cache probe.

The exact 30-second restart scenario passed alone outside the sandbox. The
fixture now probes with harmless overlay navigation until one event reaches the
reducer, records that receipt sequence, requires later reducer progress after
Escape, and only then checks the durable cache. These are separate readiness,
acceptance-progress, and durability oracles under one two-second deadline. An
intermediate single-probe fixture reproduced rejected navigation immediately
after visible text; exit 97 was added by this investigation to name that
readiness failure.

## Repair

The dedicated PTY command already serializes real PTY fixtures because they own
process-wide terminal resources. The canonical nextest gate did not preserve
that rule for the three reported cases. Exact nextest overrides now grant those
cases all test threads. This is a targeted mitigation, not a broader policy for
other capture workflows. No product deadline, lifecycle assertion, product
ordering, persistence path, cleanup path, or restoration path changed.

The capture assertion now translates its stable driver exit codes into named
stages. This makes a future failure distinguish an absent input receipt, capture
authority, published owner control, and the bounded shutdown oracle without
retaining user content or machine paths.

## Remaining uncertainty

The original exit 87 and exit 96 outcomes were not reproduced on this lane's
clean base outside the sandbox. The retained logs do not contain timestamps for
every driver stage, so they cannot prove which runnable process lost scheduling
time or exclude the reporting worktree's layout changes. The repair aligns the
reported cases with the existing serial PTY rule and removes the known visual
readiness race. It does not claim a product lifecycle defect was fixed.

An interrupted later serial gate last reported the recovery export fixture,
whose Rust test waited unboundedly for its Expect driver. That fixture and its
empty-session setup now use the existing owned-child watchdog. A post-readiness
hang injection proves the watchdog's complete reported cleanup outcome and the
registered child PID's absence. Its isolated bounded run passed; this does not
establish the cause of the interrupted gate.

## Subsequent cache and migration-cohort failures

The first attempt of PR 114 CI run 35439442832 failed on Ubuntu in
`concurrent_release_refresh_and_external_reconcile_preserve_pending_authority`.
The release refresh returned `update state lock remained busy`. The fixture
gave two writers an uncontrolled simultaneous start and required both calls to
succeed immediately. The real state lease includes atomic file replacement,
file sync, and directory sync, while a competing writer has a bounded 100-attempt
lock policy. The log establishes exhausted contention, but does not separate
filesystem latency from scheduler delay. The successful rerun did not repair
that test assumption.

The replacement test now coordinates the real acquired and contended OS-lock
checkpoints through per-store, test-only channels. Both mutation orderings must
preserve the exact pending authority. A separate test deliberately holds the
lease through the unchanged retry budget, requires the exact busy failure and
unchanged durable bytes, then proves an explicit retry after release. No
production retry count, delay, cache authority, or durability operation changed.

Main run 35440820280 at `e355db5` failed the historical 15-owner cohort with
15 restart requests and 14 accepted acknowledgements. The PR and merge had
identical Git trees. That count alone does not establish whether a replacement
failed: a reply can be missing independently of process replacement. The first
counter assertion previously omitted the execution and participant evidence.

The fixture also had a concrete lifetime mismatch: its Expect driver exited
after 20 seconds, while the historical coordinator permits a fresh 45-second
post-install preparation window and its Rust wrapper permits 90 seconds for
coordination. A new synthetic owner regression failed against the unchanged
driver after 21 seconds with `replacement cohort exited before completion`.
It passed after replacing that independent cutoff with the existing Rust PTY
watchdog. This proves the harness defect, not its causality in the original
14-of-15 result.

The cohort's absolute watchdog budget composes the existing owner startup,
coordinator, installer/probe, verification, and normal-stop bounds. Driver
liveness is checked throughout the historical coordination wait. Registered
descendants, process-group fallback, and output readers retain bounded cleanup.
An injected stuck workflow proves complete watchdog cleanup rather than merely
observing that the Rust caller returned.

A test-owned gateway wrapper is compiled against the same pinned historical
source. It preserves every response while recording exact synthetic participant
identity, stage, elapsed time, process/endpoint presence, and closed reply or
transport classifications. Accepted acknowledgement is never labeled as a
verified replacement. Before assertions and during panic unwinding, bounded
reports retain allowlisted execution counters, runtime identity, trace events,
and diagnostic stage counts under `target/qualification-evidence`. They exclude
thought content, databases, terminal output, arbitrary error strings, and paths.
Normal fixture state remains temporary and is removed after reporting.
Failed CI test, PTY, full-MSRV, and coverage jobs retain only the matching
`migration-*.json` reports as GitHub Actions artifacts for seven days. The
upload does not include fixture roots, databases, or raw process output.

The historical failure remains causally unresolved unless a reproduced trace
connects a driver exit, refusal, or transport failure to its missing receipt.
Production coordination deadlines and behavior are intentionally unchanged.
