---
name: claude-code-review
description: Run a general read-only implementation review with Claude Code as a native background subagent from Claude Code, or through the non-interactive Claude Code CLI from another harness. Never create a Herdr pane, tab, workspace, or agent for a reviewer. Do not use for Proqi's repository-wide architecture review.
---

# Claude Code Review

Run one dependable Claude Code review of a bounded implementation. Use the
invoking harness's native subagent when it is Claude Code, or the managed CLI
route from another harness. This skill is a general review lane. It does not
replace or modify `$architecture-review`.

## Establish the review snapshot

1. Resolve the repository, worktree, pull request, diff, or files the user wants
   reviewed. Read every applicable `AGENTS.md` and the relevant product or
   architecture contracts before composing the assignment.
2. Record the absolute working directory, current branch, exact `HEAD`, and
   `git status --short` before dispatch. Preserve unrelated changes.
3. Determine the requested Claude model. Use the user's requested model exactly.
   When none is specified, use `fable`, the proven default for this workflow.
4. Treat an actively changing worktree as a moving snapshot. A recorded `HEAD`
   identifies the commit, but uncommitted files can still change during review.
   Compare the revision and working-tree state again after completion. If the
   reviewed files changed, label the report as a moving snapshot and rerun after
   the fixes or stable head when a final verdict is required.

Do not silently broaden a focused review into a repository-wide architecture
audit. Use `$architecture-review` for that separate workflow.

## Prepare durable output

Create one collision-free result file under `/private/tmp` before launching
Claude. Prefer a descriptive name such as:

```text
/private/tmp/claude-code-review-<repository>-<unique-id>.txt
```

Record the exact path in the working notes immediately. The result file is the
authoritative output artifact. Do not use `claude --bg`, Claude background
session logs, or its control daemon as the only output source. The daemon can
disappear after the review completes and make otherwise successful output
unrecoverable.

## Build a strictly read-only prompt

The review prompt must contain all of the following:

- the exact review scope and recorded `HEAD`;
- the relevant repository contracts and acceptance criteria;
- an explicit instruction to inspect and report only;
- an explicit prohibition on editing, creating, deleting, renaming, formatting,
  or otherwise mutating files;
- an explicit prohibition on commands that change repository, process, network,
  GitHub, or other external state;
- a request for severity-ordered findings with concrete file and line evidence;
- a request to state when no finding exists rather than inventing one;
- a reminder that the tree may be changing and that the report must identify
  evidence observed during its own snapshot.

Use wording at least as strict as:

```text
This is a read-only code review. Do not edit, create, delete, rename, format, or
otherwise mutate any file. Do not run commands that change repository or
external state. Inspect the requested scope and return findings only.
```

Claude may use `Bash` only for read-only inspection such as `git diff`, `rg`,
and test-result inspection. The prompt prohibition remains mandatory because
the CLI tool allowlist itself does not make Bash read-only.

## Dispatch through the harness-native route

Claude must run as a non-interactive background reviewer relative to Herdr.
Never create or split a Herdr pane, tab, workspace, or worktree for it, and never
use `herdr agent start` or `herdr agent prompt` to host the review. A Claude Code
agent hosted inside Herdr is still running in the Claude Code harness. Herdr is
only the terminal host and does not change the harness identity.

When the invoking harness is Claude Code, use exactly one native Claude Code
subagent through its agent or task mechanism. Never launch the `claude`
executable from a Claude Code agent. If that agent cannot spawn a native
subagent, report the blocker to its parent session. Do not fall back to the CLI,
a Herdr reviewer, or primary-agent review.

Only when the invoking harness is not Claude Code, run the CLI inside the
implementation owner's existing worktree through the invoking harness's managed
process runner.

When this skill is running inside Codex, launch the Claude CLI outside the
Codex filesystem/process sandbox from the first attempt. Use the process
runner's explicit unsandboxed or escalated execution mode. Claude Code's local
authentication state is unavailable inside the sandbox, which can produce a
false `Not logged in` result and an unnecessary request for credentials. Do not
ask the user to log in unless the same invocation also fails outside the
sandbox. Unsandboxed execution grants access to the existing local Claude
session only; it does not relax the read-only prompt, tool allowlist, or review
scope.

Use the proven invocation shape, substituting the requested model, prompt, and
recorded result path:

```text
claude --print --model fable --permission-mode dontAsk \
  --tools Read,Grep,Glob,Bash -- REVIEW_PROMPT \
  > /private/tmp/claude-review-result.txt 2>&1
```

Pass the prompt as one safely quoted argument after `--`. Never interpolate
untrusted repository text as shell syntax. Redirect both stdout and stderr to
the result file from process start.

Run a permitted CLI command as a managed foreground process through the harness
process runner. It may continue after the initial tool call yields, but do not
detach it with shell `&`, `nohup`, `claude --bg`, or a Claude-managed background
session.
Retain the returned process or session identifier and poll that same managed
process until it exits. Here, background means non-interactive relative to
Herdr, not an unowned detached shell process. Redirected output can make a
healthy review appear silent, so silence is never evidence of completion.

## Collect and assess the review

1. Record the process exit status. Read the complete result file after the
   process exits, including when the exit status is nonzero because diagnostics
   are captured there too.
2. Re-record `HEAD` and `git status --short`. Compare them with the starting
   snapshot and inspect whether reviewed files changed during the run.
3. Verify every finding against the current source. Treat Claude's report as
   evidence and each finding as a hypothesis, not as authority.
4. Classify findings as confirmed, partially confirmed, rejected with evidence,
   already fixed during the moving review, or requiring a product decision.
5. If fixes are made, rerun focused verification and rerun the Claude review on
   the stable resulting head when the user requested a final review verdict.

Report the requested model, reviewed scope, starting and ending revisions,
moving-snapshot status, managed process outcome, and exact persistent output
path. Do not claim a stable-head review when the tree changed underneath it.

## Retain and clean the artifact

Keep the result file until its findings have been read, verified, and
incorporated into the implementation or review synthesis. A handoff that still
depends on the raw report is not complete cleanup authority.

Before deleting it, inspect the exact recorded path and confirm that it is the
owned review artifact. Remove only that file, never a glob or broad temporary
directory. State when it was removed. If ownership is uncertain or findings
have not yet been incorporated, retain it and report the path.
