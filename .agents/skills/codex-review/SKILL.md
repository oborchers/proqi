---
name: codex-review
description: Run a bounded read-only implementation review with Codex as a native background subagent from Codex, or through the non-interactive Codex CLI from another harness. Never create a Herdr pane, tab, workspace, or agent for a reviewer.
---

# Codex Review

Run one independent Codex review without turning the reviewer into an
interactive terminal participant. The implementation owner remains the only
interactive coding agent in its Herdr worktree.

## Establish the review snapshot

1. Resolve the repository, worktree, pull request, commit, diff, or files in
   scope. Read every applicable `AGENTS.md` and relevant product or architecture
   contract before composing the assignment.
2. Record the absolute working directory, branch, exact `HEAD`, requested base,
   and `git status --short`. Preserve unrelated changes.
3. Treat uncommitted files as a moving snapshot. Compare `HEAD` and status again
   after the review. A final stable-head verdict requires a fresh review if the
   reviewed files changed during the pass.

## Reviewer execution is never interactive

A reviewer must run as a non-interactive background worker relative to Herdr.
Never create or split a Herdr pane, tab, workspace, or worktree for it. Never
use `herdr agent start`, `herdr agent prompt`, or another terminal multiplexer
to host a reviewer. The reviewer inspects the implementation owner's existing
worktree.

Determine the invoking harness before dispatch. A Codex agent hosted inside
Herdr is still running in the Codex harness. Herdr is only the terminal host and
does not change the harness identity.

When the invoking harness is Codex, use exactly one native Codex subagent
through the harness's subagent or collaboration mechanism. This is mandatory.
Do not run the review in the primary agent. Never launch the `codex` executable,
including `codex exec` or `codex exec review`, from a Codex agent. If that Codex
agent cannot spawn a native subagent, report the blocker to its parent session.
The parent Codex session may dispatch the native reviewer into the same existing
worktree. Do not fall back to the CLI, a Herdr reviewer, or primary-agent review.

Only when the invoking harness is not Codex, launch one ephemeral,
non-interactive Codex CLI process through that harness's managed process runner.
Use the exact requested model when one is given. Otherwise retain the configured
Codex default. Use the review worktree as `--cd`, select the read-only sandbox,
and capture complete standard output and standard error in one collision-free
file under `/private/tmp`. A representative shape is:

```text
codex exec --ephemeral --sandbox read-only --cd REPOSITORY_ROOT \
  --color never REVIEW_PROMPT > RESULT_FILE 2>&1
```

The process may continue after the runner initially yields, but it must remain
owned by that managed process session until exit. Do not use shell `&`, `nohup`,
an interactive Codex session, or a dedicated terminal or Herdr pane.

## Give a strictly read-only assignment

The review prompt must include:

- the exact scope, base, and recorded `HEAD`;
- relevant contracts and acceptance criteria;
- an instruction to inspect and report only;
- a prohibition on editing, creating, deleting, renaming, formatting, or
  otherwise mutating tracked or untracked repository files;
- a prohibition on commits, pushes, pull-request changes, comments, reviews,
  process control, and other external mutations;
- severity-ordered findings with concrete file and line evidence;
- a request to report no findings plainly when none are supported.

The reviewer may run read-only inspection commands and non-destructive tests
when they materially validate a finding. Tests may create ordinary ignored
build or temporary output, but the reviewer must not update snapshots, fixtures,
lockfiles, generated sources, or repository state. It must never weaken a test
or gate.

## Verify the result

1. Wait for the native subagent or managed CLI process only when its result is
   needed. Record its completion status. For a CLI review, read the complete
   result file even when the process exits nonzero.
2. Re-record `HEAD` and `git status --short` and classify the result as stable or
   moving.
3. Verify every finding against the current source. Treat it as a hypothesis,
   not authority. Classify it as confirmed, partially confirmed, rejected with
   evidence, already fixed during a moving review, or requiring a product
   decision.
4. The reviewer never implements fixes. The implementation owner may fix
   confirmed findings when its assignment authorizes that work. A later pass
   must use a fresh subagent or fresh CLI process.

Report the reviewed scope, exact revisions, execution route, moving-snapshot
status, process outcome, and verified findings. Keep a CLI result file until its
findings are incorporated. Inspect its exact path before removing only that
owned artifact.
