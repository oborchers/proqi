---
name: architecture-review
description: Run Proqi's deliberate repository-wide architecture audit with independent Codex and Claude Fable reviews, synthesize evidence, and complete a behavior-preserving refactor on main. Use for a global architecture pass, not an ordinary feature or pull-request review.
---

# Architecture Review

Re-run Proqi's full architecture-review workflow without turning it into a
feature project. The default is the original end-to-end workflow: independent
parallel reviews, evidence reconciliation, a behavior-preserving refactor on
`main`, instruction updates when justified, complete verification, commit,
push, and green CI. If the user explicitly asks for review-only output, stop
before repository edits, commits, or pushes.

## Establish authority and a safe base

1. Confirm this is the Proqi repository by locating `Cargo.toml`, `AGENTS.md`,
   `context/PRODUCT.md`, and `context/ARCHITECTURE.md` at the repository root.
2. Read every applicable `AGENTS.md`, then read `context/PRODUCT.md` and
   `context/ARCHITECTURE.md` completely before reviewing boundaries or changing
   code. Read [the review rubric](references/review-rubric.md) completely.
3. Record the current branch, exact `HEAD`, upstream, worktree list, status, and
   relevant active pull requests. Preserve every unrelated tracked or untracked
   user change. Never stash, reset, clean, overwrite, or absorb it into the
   architecture work.
4. The implementation phase runs directly on `main`, matching the original
   workflow. Before editing, require local `main` and `origin/main` to agree and
   require a clean tracked tree except for changes the user explicitly assigned
   to this review. A dirty or divergent checkout does not prevent read-only
   review, but it blocks implementation until the owner resolves it.
5. Do not remove, disable, redesign, or add product features. Visible behavior,
   public Rust paths, CLI/JSON spelling, durable encodings, snapshots, failure
   classification, and Herdr contracts remain unchanged unless the user
   separately authorizes a contract change.

An unqualified explicit invocation of this skill authorizes the complete local
review/refactor workflow and its ordinary commit, `main` push, and CI monitoring
steps. It does not authorize a release, tag, package publication, repository
setting change, destructive cleanup, or unrelated work. A user instruction such
as “review only,” “do not push,” or a narrower scope always wins.

## Run independent reviews in parallel

Use exactly one native subagent from the invoking harness and one CLI-dispatched
reviewer from the other harness. Never launch the invoking harness through its
own CLI, never replace the native subagent with a second external process, and
never run two reviewers from only one model family. Both reviewers inspect the
same exact base without seeing each other's findings or a proposed answer. The
primary also performs its own repository inspection; reports are evidence
leads, not authority.

### When the primary is Codex

1. Spawn one native Codex subagent through Codex's subagent/collaboration
   mechanism. Give it the exact base, the complete repository rubric, and a
   strict read-only architecture-review assignment.
2. In parallel, dispatch Claude Code CLI using the project agent
   `.claude/agents/proqi-architecture-reviewer.md`, the Fable model, plan
   permissions, no session persistence, and captured final output. A
   representative invocation is:

   ```text
   claude -p --model fable --agent proqi-architecture-reviewer \
     --permission-mode plan --no-session-persistence
   ```

Do not dispatch `codex exec` from a Codex primary. The native Codex subagent is
the Codex review lane.

### When the primary is Claude Code

1. Spawn the project `proqi-architecture-reviewer` as a native Claude Code
   subagent through Claude's agent/task mechanism. Its checked-in definition
   pins the Fable model and plan permissions.
2. In parallel, dispatch one ephemeral Codex CLI review with the repository
   mounted read-only, for example:

   ```text
   codex exec --ephemeral --sandbox read-only -C REPOSITORY_ROOT -
   ```

   Provide the exact base and complete rubric through standard input or an
   owned temporary prompt file, never by interpolating untrusted repository text
   into a shell command.

Do not dispatch Claude CLI from a Claude primary. The native Fable subagent is
the Claude review lane.

### Shared dispatch rules

- Dispatch both lanes before waiting for either so they run concurrently.
- Ask each lane for one complete independent review rather than splitting
  ownership into unrelated micro-audits. A reviewer may use read-only helpers
  for evidence gathering when supported, but it returns one reconciled report.
- Supply a concise instruction to audit the exact recorded revision using the
  repository rubric and return findings without editing. Do not put source,
  secrets, credentials, or the other reviewer's conclusions in command-line
  arguments.
- CLI-dispatched harnesses may require network access. When the current sandbox
  restricts it, request normal outside-sandbox authorization rather than
  treating a network failure as an architectural result. Use existing user
  authentication and never print, copy, or persist credential values.
- If the opposite harness is unavailable, exhaust its ordinary authenticated
  read-only launch path, report the exact blocker, and continue the primary and
  native-agent inspections. Never fabricate a second review.

## Inspect and reconcile

Inspect the repository directly while the independent lanes run:

- map the folder tree, domain/application/port/adapter/UI ownership, public
  import paths, durable schemas, and composition roots;
- inspect representative implementation and tests rather than inferring design
  from names;
- use history and blame selectively to distinguish intentional seams from
  accidental extraction or linter-driven movement;
- locate semantic duplication, magic compatibility strings, oversized
  responsibilities, dead paths, and tests whose placement obscures ownership;
- trace shared rendering, measurement, truncation, hit geometry, cursor,
  selection, folds, and annotation behavior through all consumers.

For every external finding, verify the cited code and contract yourself.
Classify it as confirmed, partially confirmed, rejected with evidence, or a
product decision requiring Oliver. Resolve disagreement from source evidence,
not majority vote. Prefer no change over an abstraction without a clear owner or
demonstrated drift risk.

Produce one ordered refactor plan with dependencies. Separate behavior-neutral
architecture work from feature ideas and optional aesthetic preferences. Stop
for user direction when a finding requires visible behavior, removal of a
feature, a durable/public contract change, or a substantial prerequisite whose
scope no longer resembles an architecture cleanup.

## Refactor directly on main

Implement only confirmed behavior-neutral improvements:

- move responsibilities to the innermost correct owner and preserve canonical
  public re-exports where required;
- consolidate semantic rules and compatibility vocabularies at one source of
  truth, removing superseded paths in the same change;
- extract reuse when at least three consumers share a rule, or when two
  consumers drifting would be incorrect or unsafe;
- split modules and tests by responsibility, never merely to satisfy line or
  complexity limits;
- move behavior-owned tests beside their owner and keep cross-layer, process,
  persistence, and PTY contracts in top-level tests;
- replace known closed-set strings with typed identifiers, enums, or constants
  at the innermost valid boundary while preserving external spellings;
- keep constructors and private fields responsible for invariants, and retain
  injected time, filesystem, process, clipboard, path, and ID boundaries;
- do not weaken enforcement, delete meaningful coverage, accept snapshots
  automatically, or compact readable code to game a gate.

When the review establishes a durable ownership rule that future contributors
must follow, update the narrowest applicable `AGENTS.md`. Add a nested file only
for a genuine domain-specific rule set. Every `AGENTS.md` must retain its sibling
relative `CLAUDE.md -> AGENTS.md` symlink. Do not turn one refactor's incidental
shape into permanent policy.

Commit coherent architecture changes on `main`. Do not mix feature backlog,
generated review reports, temporary agent output, local paths, or credentials
into the commits.

## Verify and challenge the result

1. Run focused tests while moving each responsibility. Explicitly compare
   representative snapshots and outward fixtures before and after the refactor.
2. Run `cargo xtask check`. For a global architecture milestone, also run
   `cargo xtask audit` and `cargo xtask package`; use required host permissions
   for legitimate local socket, PTY, packaging, or platform tests rather than
   weakening them.
3. Inspect the final tree, diff, public paths, test placement, line counts,
   dependency direction, and working-tree status. Prove that behavior-neutral
   claims are reflected in unchanged outward fixtures and snapshots.
4. Repeat the same harness pairing for the final pass: the invoking harness uses
   a fresh native subagent and the opposite harness is dispatched through its
   CLI. Give both reviewers the completed diff and relevant resulting files
   without the expected verdict. Reconcile and fix every confirmed regression,
   duplicate owner, misplaced test, or linter workaround they find, then rerun
   affected checks.
5. Push the reviewed commits to `main` only when the invocation authorizes the
   full workflow. Monitor the complete required CI result and fix failures
   caused by the refactor. Retry an evidenced infrastructure flake once; do not
   weaken a gate or make unrelated product changes to obtain green CI.

## Report

Return a concise architecture handoff containing:

- exact base and final SHAs;
- the independent Codex and Fable verdicts and how disagreements were resolved;
- confirmed findings, implemented moves/deduplication/test restructuring, and
  explicit keep-as-is decisions;
- instruction files added or changed and the durable rule each captures;
- focused, canonical, audit, package, final-review, push, and CI evidence;
- any unresolved blocker or product decision, without presenting it as a
  completed refactor.

Delete only owned temporary prompts and review output after extracting the
needed evidence. Never delete an agent session, worktree, user file, or review
artifact whose ownership is uncertain.
