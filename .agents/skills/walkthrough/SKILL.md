---
name: walkthrough
description: Deliver an evidence-grounded, one-item-at-a-time walkthrough of a Proqi implementation worktree or one dedicated GitHub pull request, including a live Herdr TUI demonstration when behavior is visual. Use when explicitly invoked with $walkthrough or when the user directly asks to be walked through Proqi work. Continue the active walkthrough when the user replies next.
---

# Walkthrough

Explain and, when appropriate, demonstrate one bounded body of Proqi work one
concise item at a time. Resolve the source from context, prepare the complete
agenda privately, and reveal only the current item.

## Resolve and verify the source

1. Infer the source from the request and recent conversation before asking the
   user to repeat anything. Support:
   - the implementation in the current or named linked worktree; or
   - one specified GitHub pull request, open or merged.
2. Inspect the applicable `AGENTS.md`, branch commits, working-tree state, and
   complete diff against the verified base. For a pull request, use read-only
   GitHub queries to verify its base, head, state, and merge commit.
3. Confirm that claimed shipped work is reachable from `origin/main`. Label an
   open pull request, an unmerged branch, or uncommitted work as unshipped.
4. Ground every claim in code, tests, snapshots, documentation, or observed
   behavior. A PR description or ticket is not proof that behavior exists.
5. Ask one focused scope question only when the source cannot be inferred after
   inspection.

Do not edit code, repository documents, GitHub, pull requests, or settings
during a walkthrough. Builds, isolated test state, and disposable demonstration
resources are the only permitted mutations.

## Select the checkout

Use a checkout that faithfully contains the selected source before presenting
item one.

- A linked implementation worktree may be reused when it contains the exact
  selected implementation. Record its initial path, branch, HEAD, status, and
  Herdr workspace or pane identifiers when available. Never remove it.
- The primary checkout may be used only for work already merged to `main` and
  present in that checkout. Treat it as source-only for every unmerged change.
- For an open PR or other committed unmerged source without a matching linked
  worktree, fetch the exact head and create a collision-free detached temporary
  worktree. Record its parent directory, checkout path, and head SHA. Do not
  create a branch for a walkthrough.
- If an unmerged demonstration depends on primary-checkout changes that are not
  represented by a commit or existing linked worktree, pause and ask the user
  to commit them or move them into a dedicated worktree. Never copy, stash,
  reset, or clean user changes to manufacture a demonstration revision.

This worktree rule applies even when the walkthrough itself is invoked outside
Herdr.

## Build the private agenda

Build and freeze the complete agenda before presenting the first item. Reveal
the total count, but not future item titles.

- Group an implementation or PR into coherent user-facing behaviors, system
  contracts, or operational changes. Do not make one item per file, commit,
  test, or implementation phase.
- Classify every item privately as visual or nonvisual. A behavior is visual
  when the verified revision changes something the user can see or operate in
  the TUI. Refactors, types, tests, and CI-only changes are nonvisual.
- Do not visually demonstrate planned or described behavior unless code
  evidence confirms it exists in the selected revision.
- If evidence materially changes after the agenda is frozen, explain why and
  rebuild the agenda instead of silently changing the count or order.

## Require Herdr for visual work

Nonvisual walkthroughs may run anywhere. If the agenda contains any visual
item, require `HERDR_ENV=1` before presenting item one. When it is absent, pause
with a concise instruction to resume the walkthrough from a Proqi worktree or
coordinator session inside Herdr. Do not substitute a hidden PTY, prose-only
claim, or newly opened operating-system terminal.

Inside Herdr:

1. Load the installed Herdr skill completely when the current harness exposes
   it, then inspect the installed CLI help as the authoritative command
   contract. If that companion skill is unavailable, use the Herdr CLI help
   directly rather than blocking an otherwise capable Claude or Codex worker.
2. Reuse the selected implementation workspace when it exists. Otherwise open
   the selected temporary worktree as a Herdr workspace. Prefer a disposable
   sibling pane in the implementation agent's existing tab so the agent and
   live demonstration remain visible together. Use a new tab only when the
   current tab cannot retain usable pane geometry or the demonstration genuinely
   requires an isolated layout. Never create a new tab merely for convenience.
   Address every disposable pane or tab by an explicit ID with `--no-focus`,
   and record every resource created.
3. Build the exact checkout with `cargo build --locked --bin proqi`. Launch its
   real binary in a disposable pane with a private temporary `--state-dir`.
4. Seed only synthetic walkthrough sessions and thoughts. Never inspect or
   mutate the user's ordinary Proqi state. Disable Herdr integration unless the
   selected behavior specifically requires it.
5. Never submit to, rename, focus, or close an unrelated agent pane. A live
   integration demonstration that would create an agent session, consume model
   credentials, submit a prompt, or alter the clipboard requires the user's
   explicit authorization for that action.
6. Prepare the exact visible state before describing the item, and leave it
   visible while the user inspects or operates it. State what is selected,
   persisted, pending, or expected after the suggested interaction.

If the real TUI cannot be prepared after its documented setup and recovery
path, pause on that item and report the blocker. Do not claim a visual result
from deterministic tests alone.

## Keep strict pace

- Advance only when the user's entire reply is `next`, ignoring case,
  surrounding whitespace, and terminal punctuation.
- Treat every other reply as discussion of the current item. Answer concisely,
  keep the same item active, and end with `Say next.`
- Never show more than one agenda item in a response. Do not add back, skip, or
  overview controls.
- End each non-final item with `Say next.`
- For a final visual item, keep the TUI live and end with
  `Final item. Say next when you are finished inspecting it.` The next reply
  triggers automatic cleanup rather than another agenda item.
- For a final nonvisual item, clean any walkthrough-owned temporary checkout
  before responding and end with `Walkthrough complete.`

Use this compact shape, omitting headings that do not help:

```text
2 / 5 · Smart list continuation

What it means
<concise explanation>

Why it exists
<reason or context>

Try
<one focused interaction, for a visual item>

Expected
<observable and persistence result>

Say next.
```

Keep an item roughly 100 to 250 words. Mention paths, tests, or implementation
mechanics only when they materially improve understanding.

## Clean up automatically

The user has chosen automatic cleanup; do not ask whether to remove or retain a
walkthrough-created environment.

1. Stop only processes started by the walkthrough and close only its recorded
   test panes or tabs. Preserve implementation agents, their panes, and every
   pre-existing service.
2. Delete only the exact isolated state directory created by this walkthrough.
   Resolve and validate the recorded path before deletion; never use an
   unresolved variable, glob, repository root, home directory, or broad parent.
3. For a walkthrough-created detached worktree, verify that `HEAD` is still the
   recorded source and that tracked and untracked status is empty, then remove
   it with ordinary `git worktree remove` and no force. Remove its empty
   temporary parent afterward.
4. Never remove a reused implementation worktree. Report its path, branch, and
   status after stopping walkthrough-owned resources.
5. If any ownership check fails, the checkout is dirty, or cleanup is refused,
   retain the resource and report its exact identifiers and reason. Never force
   cleanup merely to satisfy the automatic policy.

After cleanup, report what was stopped and removed, note any retained resource,
and end with `Walkthrough complete.` If the user abandons the walkthrough,
perform the same owned-resource cleanup immediately.
