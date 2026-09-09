# CI and local validation performance spike

Status: implemented safe first slice on pull request 90. The implementation
does not change required-check policy, strict latest-main freshness, product
test selection in hosted CI, release policy, or repository settings.

Measurement baseline: `8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea`.
Hosted sample frozen after PR CI run `34281581436` completed on
2026-09-08 at 21:47:29 UTC. Local measurements were taken from the dedicated
`docs/ci-performance-spike` worktree on the baseline and are reported without
machine-specific paths.

## Recommendation

Keep the stable, fail-closed `Required CI result` check and strict latest-main
validation. Reduce cost inside that boundary rather than weakening it.

The implemented first slice is:

1. Split the platform package matrix into explicitly named Linux and macOS
   jobs. The Linux job builds and verifies the archive and Debian package once,
   and three independent image jobs consume that exact checksummed artifact.
2. Retain the current full hosted product tier. There is no expected trusted
   second owner for this repository, so no new conditional job skipping depends
   on review or another external trust control.
3. Extend the typed change classifier with package, Debian, PTY, security,
   persistence, release, and local-plan advice. CI logs this advice in shadow
   mode only. An unknown, empty, failed, or policy-changing local classification
   selects the full local gate.
4. Keep the aggregate check unconditional and make it validate the exact
   expected success or intentional-skip state for every job.
5. Make `cargo xtask check` the transparent change-aware iteration command and
   preserve the former complete gate as serialized `cargo xtask check-full`.
6. Emit stable JSON timing lines for local quality, tests, packaging, Debian
   construction, evidence validation, and container verification.

This directly addresses the measured waste. A scheduled PR run consumed a
median 3,238 job-seconds, or 53 minutes 58 seconds, while its median elapsed time
was 11 minutes 20 seconds. macOS queueing, not just execution, was a material
part of the tail. Debian was the terminal product job in 8 of 17 scheduled runs,
and PTY or macOS tests were terminal in another 7. macOS packaging and Ubuntu
tests were terminal once each.

Do not immediately remove exact post-merge validation, adopt a merge queue, or
split the test suite across more runners. Those choices need either a repository
policy decision or evidence that the added topology will beat queue and build
duplication.

## Implemented behavior

### Local commands

By default, `cargo xtask check` resolves `HEAD` and `origin/main` without
fetching, validates both as commits, and uses their merge base. `--base
<revision>` supplies another validated comparison commit. The classifier sees
committed branch changes plus staged, unstaged, deleted, renamed, and untracked
paths.

The selected plan is explicit:

| Plan | Selection | Work |
| --- | --- | --- |
| Documentation | Every changed path is ordinary Markdown and no path owns policy | changed tracked Markdown whitespace, untracked Markdown trailing whitespace, public asset validation |
| Fast | Known product or tooling paths with no high-risk class | complete `quality`, all nextest binaries except `binary(=pty)`, and doctests |
| Full | Empty, unknown, classifier failure, policy, dependency, package, or release change | the complete `quality` plus `test` gate, including PTY |

The documentation plan intentionally does not require a clean worktree. Its
purpose is to validate the changed documentation, including untracked files,
without rejecting unrelated local build outputs. Every invocation prints the
base, head, classes, selection reason, commands, omissions, outcome, and elapsed
time. It also writes the ignored local receipt
`target/xtask/check-receipt.json`. A non-full receipt says `ITERATIVE GATE ONLY`.

`cargo xtask check-full` is the explicit canonical final local command. A
`check` invocation that fails closed to Full runs the same complete locked gate,
and its successful receipt can also qualify. Both keep the former `quality`
followed by complete `test` behavior and acquire one operating-system file lock
inside the repository's trusted Git common directory. Linked worktrees
therefore wait rather than compete for CPU and I/O. The lock records only
schema, process ID, branch, and start time, reports waits, and is released by the
operating system when the process exits.

### Hosted package topology

`Package contract (ubuntu-22.04)` builds and verifies the Linux archive,
constructs and statically verifies the Debian package, and uploads the archive,
package, checksum, and evidence manifest as one short-lived workflow artifact.
`Package contract (macos-15)` remains independent. Debian verification is a
three-cell, fail-fast-disabled matrix for Ubuntu 22.04, Ubuntu 24.04, and Debian
bookworm. Every cell downloads the same producer artifact, verifies the archive,
package, embedded binary, checksum, and evidence identities, then runs exactly
one digest-pinned image contract.

The required aggregate names both platform package jobs and the complete Debian
matrix, so a failed or absent producer, artifact check, or image cell fails
closed. The existing documentation-only and product conditions are unchanged.
The new classifier fields are logged but never referenced by a workflow `if`.
Workflow permissions, pinned actions, strict required-status freshness, admin
bypass, and repository settings are unchanged.

## Scope and method

### Hosted CI sample

The sample is the 20 most recent completed `pull_request` runs of
`.github/workflows/ci.yml` available when collection was frozen. The creation-time
cutoff is inclusive, from 2026-09-08 09:42:00 UTC through 2026-09-08 21:36:58
UTC. It contains 12 successful, 3 failed, 3 cancelled, and 2
`action_required` workflow runs.

Seventeen runs scheduled jobs. The two `action_required` runs and one failed
run with no jobs represent approval or scheduling state, not runner compute.
They remain in the 20-run outcome sample but are excluded from job-duration
statistics. The 17 scheduled runs all classified as product changes. None was
documentation-only. Coverage ran in 16 and was skipped for one action-only
Dependabot change. Eight selected the full MSRV path, including one whose full
suite was cancelled before execution.

The collection used the GitHub Actions REST endpoints for workflow runs, run
attempts, and jobs. For each job:

- queue time is `started_at - created_at` when both timestamps exist;
- execution time is `completed_at - started_at`;
- workflow elapsed time is `updated_at - run_started_at`;
- executed job time is the sum of positive job execution durations;
- the critical terminal job is the last completing positive-duration job other
  than the aggregate result.

Cancelled jobs with no start timestamp have no measured execution duration.
Skipped jobs are counted but assigned no duration. GitHub does not expose a
runner CPU utilization series through these endpoints, so job-seconds are a
compute proxy, not billed-minute or CPU-second data.

The frozen run IDs are sufficient to reproduce every source timestamp without
committing transient JSON:

```text
34281581436 34281074410 34278443936 34277903360 34277692229
34277086549 34277033116 34275242953 34275013283 34274767214
34272529786 34272165146 34262555080 34258199943 34257925115
34257825676 34245843128 34241686309 34212461271 34211434345
```

Reproduction query, repeated for each ID:

```sh
gh api 'repos/oborchers/proqi/actions/runs/RUN_ID'
gh api 'repos/oborchers/proqi/actions/runs/RUN_ID/jobs?per_page=100'
```

### Local sample

The local probe ran in a dedicated worktree using Rust 1.98.1, Cargo 1.98.1,
and cargo-nextest 0.9.143 on Apple silicon macOS 26.6.1. The first probe
intentionally used a small real xtask command in a worktree without an existing
target cache:

```sh
/usr/bin/time -lp cargo xtask assets
```

The output oracle was exit status zero plus `assets valid`. A separate live
GitHub probe required run `34281581436` to report 14 job records and a successful
aggregate check. Both probes passed. The final local `cargo xtask check`
attempts and their phase timestamps are recorded in
[Qualification evidence](#qualification-evidence).

No hosted or local benchmark was run concurrently by this lane with another
known full Proqi gate. This reduces, but cannot eliminate, background runner,
network, filesystem, and machine-load variance. The sample supports topology
and order-of-magnitude decisions, not a billing forecast.

## Measured baseline behavior

### Workflow topology

At the requested baseline, the `changes` job in `.github/workflows/ci.yml` is
unconditional. It computes
`docs_only`, `coverage`, and `full_msrv`, after which the current workflow
behaves as follows:

| Change class | Jobs that execute | Jobs intentionally skipped |
| --- | --- | --- |
| Documentation-only | classifier, docs, aggregate | quality, both OS test jobs, MSRV, registry crate, security, PTY, coverage, both platform package jobs, Debian |
| Workflow-only | classifier, quality, both OS test jobs, full MSRV, registry crate, security, PTY, both platform package jobs, Debian, aggregate | docs, coverage unless Rust or a root Cargo/toolchain path also changed |
| Product without Rust-sensitive paths | full product topology except coverage; simple or full MSRV depends on paths | docs, coverage |
| Rust or root Cargo/toolchain | full product topology including coverage | docs |

Documentation-only means a nonempty diff in which every path ends in `.md`,
excluding `.github/release-notes/`. An empty diff fails closed. Workflow changes
force full MSRV. The classifier source itself is a Rust path, so changing it
triggers coverage but does not currently select full MSRV. That asymmetry should
be closed before the classifier gains more policy authority. The aggregate job
is unconditional and checks the exact expected success or skipped state of all
upstream jobs.

Consequently, documentation-only PRs do **not** run unit, PTY, package, or
security suites today. Workflow-only PRs **do** run unit, PTY, package, Debian,
security, and full-MSRV work. The latter is conservative but defensible because
the changed workflow can alter the classifier, permissions, job graph, and
required result itself. Any narrower workflow-only tier needs a trusted,
fail-closed policy boundary that the same PR cannot silently bypass.

The active `main` ruleset requires the single `Required CI result` context and
uses strict required-status freshness. It also prevents deletion and
non-fast-forward updates. This report treats those as current contracts, not
optimization targets.

The ruleset does not currently require an approving review or bind that context
to an immutable workflow identity. The workflow, classifier, and aggregate are
all controlled by the PR revision. A PR can therefore change the logic that
decides what is required and the logic that reports the required context.
In-workflow fail-closed defaults and unit tests are necessary, but they are not
a trust boundary against the same PR.

Before any future conditional skipping influences the required result, it would
need an external control:

| Alternative | Benefit | Cost or limit |
| --- | --- | --- |
| Require one non-author approval and code-owner approval for the workflow, local actions, classifier, and aggregate-policy paths | Uses repository-native review and makes policy changes explicit | Human control; requires ruleset and ownership changes and does not make the workflow immutable |
| Require a centrally managed or default-branch-owned workflow/check that the PR cannot replace | Strong automated workflow identity and consistent policy | Higher setup and repository-setting cost; untrusted PR code must never run with a write token |
| Publish the aggregate from an independently operated GitHub App | Strongest separation from PR-controlled YAML | Highest operational and credential-management burden |

Repository decision: no trusted second owner is expected in the foreseeable
future. Retain the owner's existing admin bypass, but do not treat self-review
or bypass as an external control. Keep full hosted product CI rather than
granting the PR-controlled classifier authority to skip newly conditional jobs.
Revisit a centrally managed required workflow or independently operated check
only if the ownership model changes. No repository setting is changed here.

### Local gate ownership

At the requested baseline, `cargo xtask check` was sequential and owned the
canonical local gate:

1. `quality` runs formatting, whitespace, source limits, snapshot checks,
   release highlights, assets, architecture and policy checks, Clippy, and
   rustdoc warnings.
2. `test` runs cargo-nextest for the full workspace with all features, then Rust
   documentation tests.

That shape is correct for final qualification. Its cost is poorly suited to
every edit loop, especially across isolated worktrees whose default Cargo
target directories do not share compiled artifacts.

## Hosted measurements

### Workflow sample

`Wall` starts at the workflow's `run_started_at`. `Job time` is summed execution
time and therefore intentionally exceeds wall time when jobs run in parallel.
`Critical terminal job` records the final non-aggregate job and its exact start
and completion timestamps.

| Run | Created UTC | Branch | Result | Wall | Job time | Critical terminal job, start to completion UTC |
| ---: | --- | --- | --- | ---: | ---: | --- |
| 353 | 21:36:58 | `chore/dependabot-auto-merge` | success | 10m31s | 56m01s | PTY macOS, 21:38:39 to 21:47:21 |
| 352 | 21:31:27 | `feature/mouse-capture-opt-out` | success | 11m25s | 56m37s | Debian, 21:40:25 to 21:42:47 |
| 350 | 21:04:08 | `chore/dependabot-auto-merge` | success | 19m16s | 54m39s | Debian, 21:21:07 to 21:23:18 |
| 349 | 20:58:41 | `fix/board-insert-density-scroll` | failure | 25m33s | 47m00s | Debian, 21:21:54 to 21:24:06 |
| 348 | 20:56:23 | `feature/footer-hidden-setting` | success | 20m09s | 53m58s | PTY macOS, 21:06:31 to 21:16:25 |
| 346 | 20:49:59 | `dependabot/all_dependencies-fcbeee0005` | success | 16m32s | 56m32s | Tests macOS, 20:57:54 to 21:06:26 |
| 345 | 20:49:24 | `fix/board-insert-density-scroll` | cancelled | 10m47s | 52m21s | PTY macOS, 20:51:03 to 21:00:05 |
| 342 | 20:30:55 | `fix/inbox-pause-warning-dismissal` | success | 18m29s | 51m19s | Tests macOS, 20:41:03 to 20:49:18 |
| 341 | 20:28:34 | `dependabot/all_dependencies-8b8bc88e31` | success | 17m20s | 1h05m14s | Debian, 20:42:29 to 20:45:49 |
| 340 | 20:26:00 | `fix/inbox-pause-warning-dismissal` | cancelled | 6m30s | 38m03s | Tests Ubuntu, 20:27:10 to 20:32:24 |
| 338 | 20:03:14 | `fix/herdr-sentinel-open-issues` | success | 16m53s | 1h02m11s | Debian, 20:16:42 to 20:20:00 |
| 337 | 19:59:33 | `fix/herdr-sentinel-open-issues` | cancelled | 4m39s | 27m11s | Package macOS, 20:01:16 to 20:04:06 |
| 335 | 18:21:33 | `fix/expanded-thought-up-boundary` | success | 10m50s | 53m03s | Debian, 18:30:18 to 18:32:15 |
| 333 | 17:37:10 | `feature/mouse-capture-opt-out` | action required | 0s | 0s | no jobs scheduled |
| 332 | 17:34:29 | `feature/herdr-protocol-21-22-qualification` | success, attempt 2 | 10m19s | 55m41s | PTY macOS, 17:36:22 to 17:45:12 |
| 331 | 17:33:31 | `feature/mouse-capture-opt-out` | action required | 0s | 0s | no jobs scheduled |
| 329 | 15:36:31 | `feature/herdr-protocol-21-22-qualification` | failure | 2h12m | 0s | no jobs scheduled |
| 328 | 14:58:02 | `feature/terminal-safe-navigation-insert` | success | 11m20s | 54m51s | Debian, 15:07:09 to 15:09:14 |
| 327 | 09:53:08 | `feature/terminal-safe-navigation-insert` | success | 10m16s | 49m40s | Debian, 10:01:08 to 10:03:15 |
| 326 | 09:42:00 | `feature/terminal-safe-navigation-insert` | failure | 10m50s | 51m04s | PTY macOS, 09:43:31 to 09:52:44 |

The 17 scheduled runs used 53,125 measured job-seconds in total, or 14 hours 45
minutes 25 seconds. Median workflow wall time was 11 minutes 20 seconds. Median
successful wall time was 16 minutes 32 seconds because cancelled and early
failure runs shorten the all-run statistic.

### Job distributions

`Timed` counts jobs with a positive observed execution duration. Queue
percentiles use jobs with derivable timestamps. The p90 uses the nearest-rank
observation and is descriptive for this small window.

| Job | Timed | Success / failure / cancelled / skipped | Median run | p90 run | Median queue | p90 queue | Total job time |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| Classify changes | 17 | 17 / 0 / 0 / 0 | 1m26s | 1m30s | 2s | 3s | 23m11s |
| Quality | 17 | 16 / 0 / 1 / 0 | 1m40s | 2m43s | 3s | 3s | 31m46s |
| Tests, Ubuntu | 17 | 12 / 3 / 2 / 0 | 4m55s | 5m14s | 3s | 3s | 1h13m29s |
| Tests, macOS | 17 | 13 / 1 / 3 / 0 | 7m50s | 8m43s | 11s | 7m03s | 2h01m33s |
| MSRV | 17 | 14 / 2 / 1 / 0 | 3m40s | 7m25s | 3s | 3s | 1h16m55s |
| Coverage | 16 | 11 / 3 / 2 / 1 | 4m11s | 6m01s | 3s | 4s | 1h07m48s |
| Security | 17 | 17 / 0 / 0 / 0 | 1m33s | 1m37s | 3s | 21s | 25m55s |
| Registry crate | 17 | 15 / 0 / 2 / 0 | 5m11s | 5m28s | 2s | 4s | 1h22m58s |
| PTY, macOS | 17 | 14 / 0 / 3 / 0 | 8m51s | 9m49s | 8s | 6m32s | 2h20m30s |
| Package, Ubuntu | 17 | 16 / 0 / 1 / 0 | 6m05s | 6m24s | 3s | 4s | 1h35m52s |
| Package, macOS | 17 | 15 / 0 / 2 / 0 | 6m21s | 10m18s | 10s | 10m07s | 1h48m40s |
| Debian | 15 | 14 / 0 / 3 / 0 | 2m12s | 3m18s | 2s | 3s | 35m52s |
| Required result | 17 | 12 / 5 / 0 / 0 | 3s | 4s | 2s | 4s | 56s |

Conclusion counts include cancelled jobs that never acquired a runner, so they
can exceed `Timed`. The documentation job was skipped in all 17 scheduled runs.

### Critical path and queueing

The critical terminal product job was Debian in 8 runs, PTY macOS in 5, tests
macOS in 2, tests Ubuntu in 1, and package macOS in 1. Linux queue times were
normally 2 to 4 seconds. macOS p90 queue times reached 6 minutes 32 seconds for
PTY, 7 minutes 3 seconds for tests, and 10 minutes 7 seconds for packaging.
Adding macOS shards without addressing queue supply can therefore increase cost
without improving latency.

Debian currently has `needs: [changes, package]`. Because `package` is a matrix,
Debian waits for both OS legs although it downloads only the Linux artifact.
Across the 17 scheduled runs, the macOS leg delayed Debian after Linux completed
by a median 80 seconds. Among successful runs the median was 103 seconds. In run
352, Linux packaging completed at 21:38:23, macOS at 21:40:22, and Debian began
at 21:40:25. This dependency is avoidable without dropping a check.

### Repeated CI around main integration

Four recent PRs had a final branch run after integrating current main and an
exact-main run after merge:

| PR | Final PR run, wall / job time | Main run, wall / job time | Combined job time |
| ---: | --- | --- | ---: |
| 84 | 342, 18m29s / 51m19s | 347, 19m40s / 55m54s | 1h47m13s |
| 83 | 338, 16m53s / 1h02m11s | 339, 14m36s / 58m12s | 2h00m23s |
| 80 | 332, 10m19s / 55m41s | 334, 10m43s / 53m50s | 1h49m31s |
| 68 | 319, 11m00s / 51m35s | 320, 10m22s / 51m41s | 1h43m16s |

The four final PR runs used 13,246 job-seconds. Their main runs used 13,177,
49.9 percent of the combined 26,423 job-seconds. This is duplicated topology,
but not identical semantics. The final PR run validates the latest-main merge
result required by the freshness policy. The main run validates the exact
squashed or merged commit in the trusted branch context and can seed trusted
post-merge evidence. Both should remain until a deliberately chosen merge-queue
or trusted-tier design replaces their guarantees.

Branch churn before the final main integration is different. PR 84's obsolete
run 340 consumed 2,283 job-seconds before cancellation, then final run 342 used
3,079. PR 83's obsolete run 337 consumed 1,631 before run 338 used 3,731. PR 88
ran 3,279 job-seconds before its later current-main run used another 3,361.
Cancellation prevents further spend but cannot recover work already executed.
The correct operational response is to finish branch-local edits first, then
integrate current main once immediately before final qualification, as the
repository policy now requires.

## Debian package contract

Run 352 is representative and had a 142-second Debian job:

| Phase | Observed time | Evidence |
| --- | ---: | --- |
| Runner setup and checkout | about 3s | job step timestamps |
| Rust cache restore | 17s | job step timestamps |
| nFPM download and checksum | less than 1s | pinned nFPM 2.47.0 was a small direct download |
| Linux package artifact download | 3s | artifact produced by the separate Linux package job |
| Compile xtask and its Proqi dependency | 19.3s | Cargo completion timestamp |
| Construct and statically inspect `.deb` | 7.1s | `debian-package` timestamps |
| Verify in Ubuntu 22.04 | about 38.5s | first container interval |
| Verify in Ubuntu 24.04 | about 27.4s | second container interval |
| Verify in Debian bookworm | about 21.6s | third container interval |
| Cleanup and remaining shell overhead | about 2s | job and step boundaries |

The Debian job does not rebuild the release binary. It reuses the uploaded Linux
package payload, then compiles xtask on a fresh runner to construct the `.deb`.
Each of three sequential Docker checks pulls its base image, refreshes apt
metadata, verifies metadata and contents, proves wrong-architecture and
missing-dependency installs fail, performs a real install and identity/state
smoke test, removes while preserving state, then reinstalls and queries a
session. These are meaningful package-contract checks. The repeated fresh image,
apt, and negative-install setup dominates the verification phase.

Safe optimization order:

1. Remove the unrelated macOS matrix dependency.
2. Benchmark constructing the Debian package in the Linux package job so Cargo
   output is reused, while keeping a checksummed artifact boundary for the
   verification job.
3. Benchmark three parallel verification jobs consuming the same immutable,
   checksummed `.deb`, with the aggregate requiring all three. This may reduce
   wall time toward the slowest image but adds runner startup and compute.
4. Run Debian on PRs that can affect Linux packaging, install behavior,
   dependencies, CLI identity, migrations, update behavior, release tooling, or
   the Debian contract. Run it on `main` or nightly as a backstop, and always at
   release-candidate and release gates.

A prepared immutable CI image can remove tool downloads and preinstall package
test dependencies. It does not automatically remove nested Docker pulls, apt
index refreshes, or supply-chain review. Prebaked apt indexes also become stale.
Defer such an image until simpler topology and artifact-reuse changes are
measured. If adopted later, pin it by digest, produce provenance, scan it, and
give it an explicit refresh owner.

## Local validation measurements

The cold small-command probe took 89.49 seconds wall time before reporting
`assets valid`. Cargo spent 88 seconds compiling the dependency graph because
the dedicated worktree had no target cache. The resulting target directory was
about 1.4 GiB. Three other active implementation worktrees held approximately
11, 16, and 19 GiB target directories. These are not additive proof of identical
objects, but they demonstrate material per-worktree compilation and storage
duplication.

Cargo's default build directory is workspace-relative. Sharing one mutable
`CARGO_TARGET_DIR` between active worktrees is not the recommended fix: it can
create Cargo lock contention and mix profiles, features, toolchains, generated
inputs, and cleanup ownership. A machine-wide compiler cache such as sccache is
the safer experiment because worktrees keep isolated Cargo state while reusable
compiler outputs are content-addressed. Measure clean, warm, hit rate, storage,
and invalidation behavior before making it the default.

Concurrent full gates are also CPU and I/O competitors even when each worktree
has its own target. More local parallelism can lengthen every gate and obscure
the final qualification result. A repository-wide advisory gate lock should
serialize only the heavy final gate, report who owns the lock and how long it
has waited, and rely on operating-system lock release after process exit. It
must not kill another lane or silently downgrade to a partial gate.

### Implemented xtask UX and ownership

The repository decision favors the primary command agents already choose:

```text
cargo xtask check [--base <commit>]
cargo xtask check-full
```

The first command implements this contract:

- compute the merge-aware diff from a required, validated base commit;
- ask the same typed classifier used by CI for a plan;
- print the base, head, detected classes, and every planned command before
  execution;
- select and run the exact Documentation, Fast, or Full plan described under
  Implemented behavior;
- fail closed to the full gate for unknown paths, classifier errors, empty or
  ambiguous history, workflow/policy changes, or an unavailable base;
- finish with human-readable phase output and a machine-readable receipt listing
  the selection reason, planned work, omissions, outcome, and elapsed time;
- mark only a successful Full plan as final qualification. Documentation and
  Fast plans explicitly remain iterative.

The canonical owner remains xtask. `ci_changes` is now the one typed
change-policy module used by hosted classification and local planning. CI still
uses only the pre-existing `docs_only`, `coverage`, and `full_msrv` decisions;
the richer fields are shadow evidence. Developers can run a single named test
while editing. Immediately before the final qualification commit or handoff,
one serialized `cargo xtask check-full` remains mandatory and prints a complete
receipt.

## Invariants and tier proposal

The optimization boundary is correctness, security, and artifact identity.
These invariants remain mandatory:

- strict validation of the final latest-main result before merge;
- an unconditional, stable, fail-closed required aggregate status;
- least-privilege workflow permissions, pinned third-party actions and tools,
  controlled cache writes, and no execution of untrusted artifacts in a
  privileged context;
- cross-platform compilation and deterministic tests for affected product code;
- migration and durable-contract fixtures for any affected persistence or
  protocol boundary;
- PTY startup, input, resize, signal, and clean-shutdown coverage for affected
  terminal, input, process, or lifecycle changes;
- reviewed snapshots for visible TUI changes;
- formatting, architecture, source limits, Clippy, docs, and policy checks;
- dependency and advisory policy for dependency, lockfile, toolchain, workflow,
  and release changes, plus a trusted recurring backstop;
- complete registry, platform, Debian, and release artifact integrity before a
  release, including checksums and provenance across job boundaries.

Recommended steady-state placement after the trusted ownership control is
effective:

| Tier | Mandatory work |
| --- | --- |
| Every PR | classifier, docs or quality as applicable, stable aggregate, final latest-main validation, workflow security checks; for product changes, Linux and macOS compile/tests and the applicable MSRV floor |
| Conditional PR | coverage for Rust; PTY for terminal/process/input/lifecycle; migration and contract fixtures for owned boundaries; crate and platform packages for manifests, public CLI/install/update/release paths; Debian for Linux package/install/dependency paths; dependency audit for dependency/security inputs; full MSRV for existing high-risk classes |
| Exact `main` | trusted aggregate backstop and the complete post-merge tier while conditional policy matures; later, at minimum all checks omitted from the PR plus trusted cache or artifact production |
| Nightly | full topology regardless of paths, cache cold-start sampling, all package targets, all Debian images, dependency policy, and drift detection |
| Release candidate and release | full clean build and test topology, all package contracts, checksums, provenance, install/upgrade/remove behavior, release fixtures, and artifact identity |

Nothing in the conditional tier may become `success` merely because the
classifier did not produce a value. The aggregate must know whether each job was
required, intentionally skipped, or unexpectedly absent and reject the last
state. During rollout, log classifier decisions and compare them with the full
post-merge or nightly result before removing any PR execution.

## Strategy comparison

| Strategy | Expected effect from current evidence | Effort | Confidence | Regression risk | Decision |
| --- | --- | ---: | ---: | ---: | --- |
| Retain every product job on every PR | Baseline, median 53m58s job time and 11m20s wall | none | high | lowest | Keep as fallback and release tier, not the long-term default |
| Risk-based jobs with stable aggregate | A typical non-package, non-terminal product PR can avoid about 28m40s of median job execution from PTY, registry, platform package, and Debian alone | medium | medium-high | medium | Shadow classification only; defer while no external trust control exists |
| Fast PR tier plus complete main/nightly tier | Removes low-signal PR work while retaining broad drift detection | medium | medium | medium | Defer while no external trust control exists |
| Merge queue | Eliminates manual repeated latest-main integration and validates queued merge groups | high, includes ruleset policy | medium | medium | Defer until merge volume justifies it |
| Build once, reuse checksummed or attested artifacts | Removes repeated release builds and Debian construction while establishing package identity | medium | high for checksummed reuse | low-medium | Implemented for Linux archive and Debian verification; measure hosted result |
| Cargo cache plus sccache | Can reduce repeated compilation across hosted jobs and local worktrees | medium | medium until hit-rate data exists | medium, especially cache trust | Pilot after cache threat model and benchmark |
| Test partitioning with nextest | Could shorten a CPU-bound suite, but adds builds, artifact transfer, queueing, and runners | medium-high | low for this sample | medium | Defer; macOS queue and package topology are larger problems |
| Change-aware local xtask command | Avoids a cold or warm full gate during iteration and makes omissions explicit | medium | high | low if never final | Implemented as `check` |
| One final full local gate | Prevents partial evidence from being mistaken for qualification | low | high | lowest | Implemented as `check-full`, behavior preserved |
| Per-repository final-gate coordination | Avoids local CPU and I/O saturation across worktrees | low-medium | high | low | Implemented for `check-full` |
| Reusable workflows | Reduces YAML duplication, not measured runner time | medium | high | low-medium | Use only when it creates a clearer trusted boundary |
| Prepared immutable CI image | Removes setup downloads but adds image supply-chain and refresh ownership | high | low-medium | medium-high | Explicitly defer |

The 28m40s estimate is the sum of current median execution for PTY, registry
crate, both platform packages, and Debian. It is an upper-bound example, not a
promise: some changes need some or all of those jobs, and hosted billing rounds
and pricing are outside this measurement.

## Implementation plan

### Immediate slice

1. Complete: keep full hosted product CI because there is no expected trusted
   second owner. The owner's admin bypass remains unchanged.
2. Complete: split package jobs by stable platform identity, construct Debian
   in the Linux producer, bind archive, package, binary, checksum, and evidence,
   and require all three image cells in the aggregate.
3. Complete: add typed classifier coverage for documentation, policy, terminal,
   persistence, dependency, package, release, and unknown paths. New hosted
   decisions remain advisory only.
4. Complete: make `check` change-aware, add `check-full`, emit transparent
   receipts, and serialize final gates across linked worktrees.
5. Complete: emit stable phase timing records. Hosted before-and-after results
   are recorded during final pull request qualification.

### Second slice

1. Keep collecting shadow classifier decisions. Do not turn them into hosted
   conditions unless a future external workflow identity or ownership model
   supplies the missing trust boundary.
2. Compare the implemented parallel Debian matrix with the 142-second baseline,
   including critical path, runner queue, and total job time. Revert to one
   sequential verifier if runner startup and repeated xtask compilation erase
   the latency benefit.
3. Consider artifact attestation only when a package crosses a release trust
   boundary. The current same-workflow checksum and evidence contract is the
   appropriate PR boundary.
4. Pilot sccache with isolated Cargo target directories. Pin the integration,
   prevent untrusted cache writes, and publish hit-rate and wall-time evidence.

### Explicit deferrals

- Do not enable new conditional skipping while the PR revision controls both
  classifier and required aggregate. With no expected trusted second owner,
  this is an indefinite deferral rather than an incomplete rollout step.
- Do not change strict latest-main freshness or remove exact-main validation in
  this performance project.
- Do not enable a merge queue until merge volume, `merge_group` workflow support,
  required-check identity, and release behavior receive a policy review.
- Do not share one mutable Cargo target directory across worktrees.
- Do not shard nextest until build archives or equivalent reuse are proven and
  macOS queue data shows more runners will reduce wall time.
- Do not move all Debian verification to nightly. Relevant PRs and every release
  candidate still require it.
- Do not use caches as an artifact-integrity boundary. Treat restored cache data
  as untrusted and regenerable.
- Do not introduce a prepared CI image until its digest pinning, provenance,
  scanning, refresh cadence, and incident owner are defined.
- Do not path-filter the entire required workflow. A skipped required workflow
  can remain pending; keep the always-running aggregate and condition jobs
  inside it.

## Risks and residual uncertainty

- The hosted window is a dense 12-hour period from one day. It includes useful
  queue pressure and cancellation behavior but may not represent quieter weeks.
- Job-seconds approximate parallel compute. They are not CPU utilization or
  invoice data.
- GitHub-hosted image, network, cache, and runner availability vary. The large
  macOS p90 needs a longer window before capacity decisions.
- Debian subphase times come from one representative successful run. The
  topology finding is deterministic, but image and apt times need repeated
  instrumentation.
- No documentation-only run appeared in the frozen 20-run window. Exact
  documentation behavior is established from the workflow and classifier tests,
  not a hosted timing sample.
- sccache savings are unmeasured. Rust compiler flags, proc macros, build scripts,
  feature combinations, and cache security can reduce or reverse the benefit.
- Conditional execution can create false negatives if ownership drifts. Typed
  ownership, fail-closed defaults, aggregate checks, and full trusted backstops
  are prerequisites, not follow-up polish.

## Primary references

- GitHub, [Dependency caching reference](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching): restore-key matching, cache immutability, branch scope, pull-request merge-ref scope, and cache security boundaries.
- GitHub, [Dependency caching concepts](https://docs.github.com/en/actions/concepts/workflows-and-actions/dependency-caching): caches are for regenerable inputs; artifacts transfer job outputs.
- GitHub, [Workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax): path filters, permissions, job conditions, and required-workflow skip behavior.
- GitHub, [Control workflow concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency): concurrency groups and cancellation semantics.
- GitHub, [About protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches): strict required checks and current-base requirements.
- GitHub, [About code owners](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners): path ownership and required code-owner review behavior.
- GitHub, [Managing a merge queue](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue): merge-group validation and the required `merge_group` Actions trigger.
- GitHub, [Store and share data with workflow artifacts](https://docs.github.com/en/actions/tutorials/store-and-share-data): passing build outputs between jobs.
- GitHub, [Artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations): provenance and integrity claims for build outputs.
- GitHub, [Reuse workflows](https://docs.github.com/en/actions/reference/workflows-and-actions/reusing-workflow-configurations): permission monotonicity, secrets, caller context, and concurrency caveats.
- GitHub, [Using secrets in GitHub Actions](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-secrets): fork, Dependabot, and reusable-workflow secret boundaries.
- Cargo, [Build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html): workspace-relative target directories and compiler-cache integration.
- nextest, [Test partitioning](https://nexte.st/docs/ci-features/partitioning/): hash and slice partitioning plus archive-based build reuse.
- sccache, [Rust usage](https://github.com/mozilla/sccache/blob/main/docs/Rust.md): `RUSTC_WRAPPER`, incremental compilation, and Rust-specific caveats.

## Qualification evidence

### Original spike baseline

The branch integrated current `main` at
`a00f55da09d8ff8922ea1cbf04ea00a0b9662acd` before final qualification. The
original measurement baseline remains the exact requested
`8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea`.

The canonical local gate was not green on macOS 26.6.1. This is material local
evidence and was not hidden, skipped, retried with a smaller test set, or fixed
outside the scope of this documentation spike. A controlled warm measurement
was run in this worktree while detached at the exact requested baseline, after
the competing heavy lane ended, with 12 GiB free and no active compiler:

| Attempt | Environment and result | Timed phase evidence |
| --- | --- | --- |
| Exact baseline | unchanged `cargo xtask check` at `8faf444cfa54cdf65f77a6f0f9cdb065a2aa32ea`; fail-fast after 1,014 passes, 3 PTY failures, and 5 skips out of 1,700 tests | 112.40s total; xtask refresh 10.29s; pre-Clippy policy work 2.73s; Clippy 7.44s; rustdoc 3.45s; nextest build 10.09s and ran 66.48s before fail-fast |
| 1 | sandboxed `cargo xtask check`; stopped when Unix socket binds returned `Operation not permitted` | 114.04s total; xtask build 4.43s; Clippy 43.22s; rustdoc 10.48s; nextest build 35.05s |
| 2 | unchanged command with normal socket access; stopped after 797 passes when the system temporary volume reported `No space left on device` | 108.91s total; xtask build 23.54s; Clippy 8.49s; rustdoc 2.77s; nextest build 17.20s and ran 37.30s before fail-fast |
| 3 | unchanged command after removing only this worktree's rebuildable incremental cache; stopped after 1,014 passes on three PTY shutdown tests | 226.68s total; xtask build 45.25s; pre-Clippy policy work 4.59s; Clippy 31.59s; rustdoc 13.20s; nextest build 42.16s and ran 75.43s before fail-fast |

The full serial repository PTY gate then passed 81 of 85 tests and reproduced
the same three screenshot-capture shutdown failures, plus one update-restart
fixture failure. The three shutdown fixtures reproduced after the competing
Docker lane ended, after three orphaned CPU-saturating Proqi test processes were
terminated, and with the ambient `HERDR_ENV` marker removed. Two fixtures lost
the final `!` edit before termination. The persistent-failure fixture exited
without its expected durable thought and reported that the input lane exceeded
the shutdown deadline. This is a repeatable local macOS 26.6.1 compatibility or
timing issue, not a report-caused diff. Exact current main passed hosted CI run
`34282558047` on macOS 15 and Ubuntu before this report was committed.

Original focused qualification completed as follows:

- all 14 cited links returned HTTP 200 after redirects;
- `cargo xtask quality` passed, including formatting, source limits, snapshots,
  assets, architecture and release policy, Clippy, and rustdoc;
- staged Markdown whitespace validation passed;
- a native Codex documentation review covered the analysis-only snapshot;
- hosted documentation-only CI passed before the original draft became ready;
- `cargo xtask audit` and `cargo xtask package` were intentionally skipped for
  the original analysis-only snapshot because it changed no dependency,
  package, release, workflow, or product path.

The local failure itself reinforces two recommendations in this report: final
gate coordination must expose ambient load and environment, and PTY coverage
must remain mandatory for relevant changes. It is not evidence for relaxing a
PTY test or timeout.

### Implementation qualification

The implementation touches workflow, xtask, package, and dependency ownership,
so it requires more than the original documentation-only qualification.
Evidence collected before the first implementation push:

| Check | Result and timing |
| --- | --- |
| xtask focused suite | 90 passed in 0.64s after a 4.48s build; classifier tests cover dirty, deleted, untracked, both sides of renamed paths, and full advice for high-risk input; lock tests cover linked-worktree identity, contention, release, and reacquisition |
| Workflow syntax and security | `actionlint` passed; offline pedantic `zizmor` passed with no findings after matrix data was moved through a fixed environment variable |
| macOS `check-full`, sandbox | quality passed; nextest stopped on 7 Unix socket binds denied by the sandbox; 29 passed, 7 failed, 5 skipped before fail-fast |
| macOS `check-full`, normal system access | quality passed; nextest reproduced the baseline-only macOS 26.6 shutdown result, 1,013 passed, 3 failed, 5 skipped, and 697 not run after fail-fast; measured gate 119.53s, including Clippy 10.41s, rustdoc 3.11s, and nextest 102.50s |
| Dependency policy | `cargo xtask audit` passed; cargo-deny reported only the repository's accepted duplicate-version warnings, cargo-audit found no vulnerability, and cargo-shear found no issue |
| macOS package contract | passed with normal socket access; warm phases were notices 10.10s, build 0.26s, archive 3.75s, archive verification 0.24s, and installed-product contract 14.40s; the prior cold dist build took 89.93s |
| Linux Docker parity, first cold attempt | quality passed; initial xtask build 3m33s, Clippy 148.07s, rustdoc 32.91s; nextest compiled and ran for 413.19s, then one unrelated process test failed because an emulated grandchild exited before its PID file was observed; the repository parity command stopped fail-closed before MSRV, audit, coverage, and package |
| Exact-main macOS `check-full` | passed in 207.16s; the lock was immediate, Clippy took 18.16s, rustdoc 8.17s, all 1,720 nextest tests passed in 158.30s with 5 skips and 1 leak report, and the doctest phase took 19.48s |
| Linux Docker parity, one warm retry | the cached image and complete stable `check-full` passed; Clippy 52.44s, rustdoc 18.59s, all 1,613 Linux nextest tests passed with 5 skips in 360.06s, doctests took 81.00s, and the stable gate took 518.25s; the later isolated Rust 1.88 full-test build was killed by signal 9 under x86 Docker emulation, so parity stopped fail-closed before audit, coverage, and package |

The first native hosted implementation run was CI run `34331094007` at
`ee7c76ed4bd45edd96a5ed38295f96cdd94f9b0b`. It completed successfully from
2026-09-09 08:47:49 UTC through 08:58:34 UTC. Its 645-second wall time included
3,718 positive job-seconds. The classifier selected product CI, logged schema 2
shadow advice, required every product job, and skipped only the documentation
job. The stable aggregate passed.

| Native hosted path | Job or step time | Result |
| --- | ---: | --- |
| Classifier | 88s job, 82s command | passed; cold xtask compilation remained visible |
| Linux package producer | 317s job | passed; archive build and verification 258s, Debian construction and static verification 31s, artifact upload 2s |
| macOS package producer | 323s job | passed independently and no longer delayed Debian fan-out |
| Debian Ubuntu 22.04 | 133s job, 104s verification step | passed; evidence 1.06s and image contract 29.46s after xtask compilation |
| Debian Ubuntu 24.04 | 128s job, 104s verification step | passed; evidence 1.39s and image contract 28.63s after xtask compilation |
| Debian bookworm | 118s job, 93s verification step | passed; evidence 1.08s and image contract 20.37s after xtask compilation |
| Debian producer-to-last-consumer path | 453s from Linux producer start | passed; the three consumers ran concurrently and used 379 aggregate job-seconds |
| Critical terminal job | 541s PTY job | passed; the required aggregate completed 3s later |

The first matrix sample removed the baseline median 80-second wait for the
unrelated macOS package leg. Its 453-second Linux-producer-to-consumer path was
about 60 seconds below the baseline median Linux/macOS package dependency plus
the 132-second Debian job. The isolated verification stage did not yet become
faster: the slowest cold matrix cell was 133 seconds versus the 132-second
baseline median, and three cells used 379 job-seconds. Each cell spent roughly
70 seconds compiling xtask before the measured evidence and image phases.
Therefore the matrix improves dependency topology and failure isolation, but
its steady-state latency and compute case still depends on repeat-run cache
evidence. If repeated native runs do not lower that compile overhead, the
second-slice recommendation remains one sequential consumer without restoring
the macOS dependency.

Immediately after this run, the branch integrated exact current `main` once at
`dd05c49bf3c1e8c1aa2cd707ed3f3b40ca2bc2b9`. The resulting merge commit was
`7a2d365d8074fce742f7a2af7eff1ab8386700ca` before the final report update.

The pre-integration macOS shutdown failures matched the exact-base measurement
and remained green on the required hosted macOS 15 runner. After integrating
current main, the complete local gate passed all three formerly failing
shutdown tests and every other nextest test. The first Linux process failure
also did not recur during the warm stable Linux gate. The later MSRV signal 9
is a local x86 Docker resource failure, not a test assertion or changed package
contract. None of these results is hidden, excluded, or used to weaken a gate.
The first native hosted Linux, macOS, package, Debian matrix, and aggregate run
is green. Final qualification still requires the bounded native Codex
correction review, final hosted aggregate success, and mergeability.
