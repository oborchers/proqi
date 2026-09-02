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
   branch, commits, and exactly one draft pull request for that ticket. The
   worker may choose and update its evidence-based title and body, apply
   relevant existing labels, monitor and repair CI, and mark the pull request
   ready once it is green, mergeable, and has no known defect. None of those
   actions require another approval. This is not permission to comment, review,
   merge, publish a release or package, change repository settings, create or
   change labels or rules, create another pull request, or broaden scope unless
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
  comments, use the user's configured Git/GitHub identity, and state that this
  invocation authorizes one draft pull request plus its later ready transition
  without another text or action approval;
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
- Treat `herdr pane send-text` as literal text injection, not as a terminal key
  event transport. Do not use it to qualify escape sequences, modifier chords,
  BackTab, bracketed paste framing, or another exact terminal protocol. When
  `pane send-keys` does not expose the required key distinctly, drive the exact
  bytes through a real PTY tool such as Expect inside the disposable pane.
- A successful pane send proves only that Herdr enqueued the input. Likewise,
  `pane wait-output` searches immediately and may match text that was already
  present. Prove each live step with a unique new screen postcondition, terminal
  transcript event, durable revision, or external-state inspection; a generic
  label such as `saved`, `ready`, or `complete` is not transition evidence.
- Define the live oracle before combining stress dimensions. Seed fixtures
  through a stable CLI or API when fixture creation is not the behavior under
  test, exercise one invariant first, and inspect its exact durable result before
  adding load, resize, repetition, Unicode, or another neighboring state. A
  compound script whose timeout could mean transport, setup, rendering,
  persistence, eligibility, or teardown is not useful completion evidence.
- Derive changed and deliberately unchanged cases from the feature contract.
  Keep supported actions separate from conservative no-ops so a correct refusal
  is not misdiagnosed as dropped input. Increase content size and repetition
  only after the minimal case proves the same path with an unambiguous oracle.
- When a PTY driver echoes child terminal traffic into a Herdr pane, terminal
  capability queries can elicit replies that remain buffered as later shell
  input. Keep child terminal traffic contained (for example, Expect
  `log_user 0`), emit only explicit text sentinels, and verify the pane is back
  at an uncontaminated shell before reusing it.
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
- Push only the topic branch. Open exactly one draft pull request against `main`
  with a concise evidence-based title and a body covering summary, acceptance
  criteria, tests, live/manual verification, risks, limitations, and follow-ups.
  Keep that body accurate as branch evidence changes. The invocation is already
  approval for the title, body, creation, and updates, so do not stop to request
  another outbound-text approval.
- Inspect the repository's existing labels and apply only relevant existing
  labels. Common Proqi choices are `enhancement`, `bug`, `documentation`,
  `rust`, and `accessibility`. Never create labels or repository rules unless
  separately authorized.

Monitor the pull request's complete CI result. Diagnose failures before retrying.
Fix failures caused by the branch and rerun the required gates. One evidence-
based retry is reasonable for an infrastructure flake; repeated unexplained
failure is a blocker, not permission for indefinite retries. Never weaken a
gate, threshold, test, or snapshot to obtain green status.

When the complete pull request is green and mergeable and no known defect or
review issue remains, mark it ready for review without requesting another
approval. Do not post comments or reviews while doing so. A ready pull request
is still only a review artifact; the worker never merges it.

## Finish and hand back

The worker is done only when the pull request is green, mergeable, and ready for
review, the full acceptance criteria are met, and no known defect or review
issue remains. It must then mark its goal complete and send a structured Herdr
handoff to the invoking pane. The original skill invocation already authorizes
this internal handoff, so the worker must not ask Oliver for separate approval
before submitting it. The handoff must contain:

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
