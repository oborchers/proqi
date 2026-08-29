---
name: implement-in-worktree
description: Orchestrate one implementation ticket in one isolated Git worktree with one Herdr-managed coding agent, one goal, verification, and a pull request. Use only when explicitly invoked from inside Herdr.
---

# Implement in Worktree

Run one autonomous implementation lane without losing the primary session's
ownership of scope, review, or merge authorization.

## Hard gates

1. Before any other action, run `test "${HERDR_ENV:-}" = 1`. If it fails, stop
   and say this skill requires execution inside Herdr. Do not substitute plain
   Git worktrees, background shells, internal subagents, or another multiplexer.
2. Confirm the invocation describes exactly one bounded ticket or story with a
   testable outcome. Ask the user only for product choices that cannot be
   discovered or safely inferred.
3. Treat the user's invocation as permission to create a worktree, worker,
   branch, commits, and a pull request for that ticket. It is not permission to
   merge, publish a release, change repository settings, or broaden scope unless
   the user says so explicitly.
4. Read every applicable `AGENTS.md`. Read repository product or architecture
   context when those instructions require it.

## Learn and capture live state

- Use `herdr --help` and the relevant `herdr worktree`, `herdr agent`, `herdr
  pane`, and `herdr notification` help before controlling Herdr. The installed
  CLI is authoritative.
- Capture the invoking workspace, tab, pane ID, repository root, current branch,
  tracked worktree state, remote, and authenticated GitHub identity. Use explicit
  IDs and `--no-focus`; never target another client's focused pane.
- Fetch remote state when authorized by the requested PR workflow. Resolve and
  record the exact base SHA. By default, require local `main` and `origin/main`
  to agree. If they diverge or tracked changes would contaminate the lane, stop
  and ask the invoking session to resolve the base. Do not disturb untracked
  user files.
- Derive a short unique topic branch, worktree label, and worker name from the
  ticket. Prefer `feature/`, `fix/`, or `docs/` according to the work.

## Create exactly one lane

1. Create one Herdr-managed Git worktree/workspace from the recorded base with
   `--no-focus`. Parse its workspace and root-pane IDs from returned JSON.
2. Start one agent in that root pane. Use Codex by default. Use Claude only when
   the user explicitly requests it or Codex is unavailable and Claude can honor
   the same persistent-goal contract.
3. Give the worker exactly one persistent goal covering the complete ticket.
   Require the worker to create it through its supported goal mechanism before
   editing and to keep it active until the pull request is ready. If the chosen
   agent cannot maintain a goal, stop rather than silently replacing it with a
   plain prompt.
4. Include the invoking pane ID in the worker brief so the worker can report
   results and blockers through Herdr.

The brief must contain:

- the task, acceptance criteria, out-of-scope boundaries, branch, base SHA, and
  worktree path;
- the applicable repository instructions and required product or architecture
  reading;
- an exploration-first phase covering relevant code, tests, contracts, history,
  and useful upstream or open-source implementations;
- permission to use read-only research subagents when valuable, while keeping a
  single implementation owner and avoiding overlapping edits;
- focused and canonical test expectations, snapshot review, and the mandatory
  live Herdr stress checkpoint below, including real API, PTY, or visual paths
  that the feature exposes, plus the prohibition against weakening gates;
- permission boundaries for credentials and external actions: use only supplied
  credentials, never copy secrets into prompts, files, logs, commits, PRs, or
  comments, and use the user's configured Git/GitHub identity;
- the pull-request and structured-handoff requirements below.

Submit the brief through `herdr agent prompt ...` without `--wait`. Dispatching
must return after Herdr accepts the prompt so independent lanes can start in
parallel; a long-running goal must not turn prompt submission into a timeout.
Use `agent get`, `agent read`, and a separate `agent wait` only for deliberate
monitoring after every intended lane has been dispatched. Inspect the agent
before responding to `blocked`, `unknown`, a monitoring timeout, or a stalled
prompt. Do not blindly resend a prompt.

## Exploration checkpoint

The worker must inspect before editing. It may proceed autonomously when the
ticket fits the existing design and can satisfy its acceptance criteria.

Stop the lane when evidence shows any of the following:

- a substantial prerequisite refactor should land on `main` first;
- an upstream API, hook, stable identity, platform capability, or permission is
  missing;
- the only available approach would be misleading, fragile, unsafe, or knowingly
  deficient;
- the requested behavior conflicts with repository contracts or needs a product
  choice outside the ticket.

Do not jump through compatibility hoops merely to produce code. Preserve the
worktree, do not open a deficient pull request, and report to the invoking pane
through Herdr with the evidence, affected acceptance criteria, attempted safe
alternatives, and recommended next action. Ask the missing question when user
input can unblock the work. Use the worker's supported blocked or stale goal
state truthfully; never mark the goal complete. If its goal system delays a
blocked transition, keep the goal active and still report immediately.

## Implement and qualify

- Implement only the ticket and necessary supporting changes. Preserve unrelated
  user work.
- Run focused tests during development and every repository-required canonical
  gate before committing. Review all snapshot or golden-file diffs explicitly.
- For live TUI, Herdr, harness, API, PTY, or visual qualification, the worker may
  create disposable test tabs and panes inside its assigned Herdr workspace.
  Address them by explicit IDs with `--no-focus`; never reuse the invoking
  coordinator's pane, another workstream's workspace, or a user's unrelated
  pane. Temporary test panes do not authorize another implementation agent,
  worktree, or parallel implementation owner.
- When the feature integrates with Herdr or a harness, exercise the real
  structured API and relevant UI/PTY behavior in the worker's Herdr workspace.
  A mock-only result is insufficient when live qualification is feasible.
- Before handoff, run the implemented feature from the exact topic-branch build
  in a disposable live pane inside the assigned Herdr workspace and actively try
  to break it. This is a completion gate, not an optional walkthrough or a
  substitute for automated tests.
- Derive an adversarial matrix from the feature's actual risks. At minimum,
  exercise applicable boundary and empty inputs, unusually large content or
  collections, overflow and narrow/shallow layouts, Unicode and control-heavy
  text, rapid or repeated input, repeated activation/deactivation, cancellation,
  resize/reflow, and restart or recovery. Repeat idempotent actions enough to
  prove that they converge on the same result without duplicate durable writes,
  deliveries, receipts, or resources. For a nonvisual feature, drive the real
  command, API, persistence, or integration path from that live pane rather than
  inventing a cosmetic TUI scenario.
- Exercise meaningful combinations with neighboring state such as editing,
  selection, collapse, scrolling, pending persistence, failure, retry, and undo
  when the ticket can interact with them. Use both keyboard and mouse paths when
  the behavior has both. Do not use private user content or perform an
  irreversible external action merely to manufacture stress evidence.
- Treat every failure found during the stress pass as implementation work:
  reproduce it with a focused automated regression where practical, fix it, and
  rerun the relevant stress case and canonical gate. An unresolved defect means
  the lane is blocked or incomplete and must be reported; it cannot be hidden as
  a residual risk while declaring the goal complete.
- Record every temporary test tab and pane as it is created. Close those
  disposable resources before handoff, remove only test state created by the
  worker, and report the live scenarios exercised plus any resource that could
  not be cleaned up. Preserve the implementation pane and worktree for primary
  review.
- Inspect the final diff, status, new files, generated artifacts, and commit
  identity. Commit coherent changes without secrets or machine-specific paths.
- Push only the topic branch. Open a pull request against `main` with a concise
  title and a body covering summary, acceptance criteria, tests, live/manual
  verification, risks, limitations, and follow-ups.
- Inspect the repository's existing labels and apply only relevant existing
  labels. Common Proqi choices are `enhancement`, `bug`, `documentation`,
  `rust`, and `accessibility`. Never create labels or repository rules unless
  separately authorized.

Monitor the pull request's complete CI result. Diagnose failures before retrying.
Fix failures caused by the branch and rerun the required gates. One evidence-
based retry is reasonable for an infrastructure flake; repeated unexplained
failure is a blocker, not permission for indefinite retries. Never weaken a
gate, threshold, test, or snapshot to obtain green status.

## Finish and hand back

The worker is done only when the pull request is green and mergeable, the full
acceptance criteria are met, and no known defect or review issue remains. It must
then mark its goal complete and send a structured Herdr handoff to the invoking
pane containing:

- pull-request URL, branch, base and head SHAs, and worktree/workspace IDs;
- implementation summary and important design decisions;
- focused and canonical verification plus a concrete stress-test matrix listing
  the live inputs, repetitions, boundary conditions, state combinations, and
  observed results; never summarize this only as "manual testing passed";
- CI status and any residual risks or intentionally deferred work;
- exact cleanup identifiers.

The worker never merges its pull request. The invoking primary session reviews
the complete diff and validation itself, and sends revisions back to the same
worker when needed. The primary merges only when the user's original request
explicitly authorized merging; otherwise it reports readiness and waits.

After a successful merge, remove only the Herdr worktree/workspace created by
this invocation, then safely delete its merged local branch with `git branch
-d`. Never use forced deletion for routine cleanup. Preserve blocked or unmerged
lanes for inspection and report their exact identifiers.
