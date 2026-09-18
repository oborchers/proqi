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
whose Expect parent had an unbounded `child.wait()`. That fixture now uses the
existing owned-child watchdog and proves registered child cleanup with a
post-readiness hang injection. Its isolated bounded run passed; this does not
establish the cause of the interrupted gate.
