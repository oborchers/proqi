---
name: proqi
description: Inspect or update one user-specified Proqi scratchpad session through the installed version's JSON CLI. Use only when the user explicitly invokes Proqi or names this skill.
---

# Proqi

Act only after explicit invocation. Use the scriptable CLI, never TUI output.

## Contract

1. Begin with `proqi --json capabilities`.
2. Require `schema_version: 1`, `ok: true`, and
   `data.cli_schema_version: 1`. If the contract is unsupported, stop.
   Before using an operation, require its exact spelling in
   `data.operations.<family>`. A broad boolean does not imply that an installed
   version supports every mutation described by this skill.
3. Address the session the user specified. If none is unambiguous, run
   `proqi --json sessions list` or add `--query <text>`, show the matches, and
   let the user choose. Never guess from recency alone.
4. Run all further commands with `--json`. On `ok: false`, surface
   `error.code`, `error.message`, and `error.details` without rewriting them.
   Do not automatically retry busy, ambiguous, conflicting, or unsupported
   mutations. Every code, its exit status, `details` shape, and retry guidance
   is listed in the
   [CLI error reference](https://oborchers.github.io/proqi/reference/cli.html#errors),
   and the installed `capabilities` response publishes the same inventory in
   `data.error_codes`. Branch on `error.code`, never on the message.
5. Pass arbitrary thought content as exact standard-input bytes. Do not put it
   in shell syntax, command arguments, environment variables, or temporary
   command files.
6. Read only what the user requested. Do not list or inspect every scratchpad
   merely because the skill is active. Never parse terminal escape sequences,
   read SQLite, inspect leases, connect to owner endpoints, or access runtime
   metadata files. The installed JSON CLI is the only application boundary.
7. Perform only the requested mutation. Preserve operation receipts so the
   user can identify and reverse the change.
8. Treat `thought_locked` as authoritative. A thought with an in-flight agent
   submission cannot be changed until its owner journals a terminal outcome.
   Do not retry or bypass that lock.
9. Do not trigger update checks. JSON commands never check implicitly. Run an
   explicit update command only when the user specifically requests one and the
   installed capabilities advertise it.

Proqi is pre-`1.0`. Discover and follow the installed JSON schema rather than
assuming that a command from another minor release remains compatible.

## Typed identifiers

Copy identifiers from Proqi JSON and keep them opaque. `ses_` identifies a
session, `tht_` a thought, `sep_` a payload-free separator, `op_` a board
operation, `rev_` an editor revision, `req_` a control request, `sub_` a
submission receipt, and `ins_` a running instance. The suffix is 26 lowercase
base32hex characters containing a complete UUIDv7. Never shorten, re-case,
fabricate, or use one prefix where another type is expected.

The examples below use these canonical fixtures:

```text
ses_06g30t7dv5qv55n1ppn3clis3k
tht_06g30t8fudrq55fdkk348i7388
sep_06g30t8fudrq55fdkjqr6mpe44
op_06g30t8fudrq55fdkjqr6mpe44
```

Replace them only with same-typed identifiers returned by the live CLI.

## Capability inventory

The bundled skill is tested against this exact operation manifest. Treat the
installed `capabilities` response as authoritative, because an older installed
version may expose only a subset.

```json
{
  "diagnostics": ["collect", "keypress"],
  "sessions": ["list", "ensure", "create", "rename", "trash", "restore", "undo", "redo", "prune"],
  "items": ["insert-separator", "move", "delete", "duplicate"],
  "thoughts": ["list", "inspect", "add", "delete", "rename", "replace", "collapse", "move", "split", "extract", "merge", "reflow", "send", "undo", "redo"],
  "update": ["check"],
  "history_scopes": ["board", "editor", "browser"]
}
```

## Examples

Discover and find sessions:

```console
proqi --json capabilities
proqi --json sessions list
proqi --json sessions list --query Unicode
```

List thoughts only when requested, then inspect one specified thought:

```console
proqi --json thoughts list ses_06g30t7dv5qv55n1ppn3clis3k
proqi --json thoughts inspect ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388
```

`thoughts list` returns `items`, the authoritative ordered mixed Board
projection. Each entry is typed. Thought entries carry `content`, `name`,
`content_sha256`, and presentation metadata; separator entries contain only
their `sep_` identity, position, and timestamps. Never treat a separator as an
empty thought. The former `thoughts` array no longer exists.

Both list commands accept `--limit N`. Their responses include `total` and
`next_after`. When `next_after` is not `null` and the user needs more, pass it
as `--after` with the same session, query, and filters. Prefer a small limit
over reading a large session completely:

```console
proqi --json thoughts list ses_06g30t7dv5qv55n1ppn3clis3k --limit 20
proqi --json thoughts list ses_06g30t7dv5qv55n1ppn3clis3k --limit 20 --after tht_06g30t8fudrq55fdkk348i7388
proqi --json sessions list --limit 10
```

List and inspect synchronize with an active session owner before reading. Both
return `content_sha256`. Use that digest as the precondition for exact
replacement so a concurrent human edit cannot be overwritten:

```text
argv:  ["proqi", "--json", "thoughts", "replace",
        "ses_06g30t7dv5qv55n1ppn3clis3k",
        "tht_06g30t8fudrq55fdkk348i7388",
        "--revision-id", "rev_06g30t8fudrq55fdkm4b0acm6g",
        "--expected-sha256", "<digest returned by inspect>"]
stdin: Exact replacement content.
```

Use `--force` only when the user explicitly asks to replace the current content
regardless of intervening edits. It never bypasses a submission lock. Exact
replacement is an editor revision and can be reversed with thought-scoped undo.
Reuse the same `rev_` identifier only when retrying the exact same replacement.

Set durable collapsed presentation explicitly when requested:

```console
proqi --json thoughts collapse ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 --collapsed true
```

Set or clear the optional organizational name independently of thought content:

```console
proqi --json thoughts rename ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 "Review notes"
proqi --json thoughts rename ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 --clear
```

Names are metadata. Do not prepend them to content or encode them as annotations.

Add exact content by direct process execution, with the prompt as standard
input rather than an argument:

```text
argv:  ["proqi", "--json", "thoughts", "add", "ses_06g30t7dv5qv55n1ppn3clis3k"]
stdin: Review the Unicode resize behavior.
```

When the user wants the new thought named, pass `--name` in the same command.
Content, name, and position then form one Board operation and one undo step, so
no unnamed intermediate thought exists. Do not add and rename separately:

```text
argv:  ["proqi", "--json", "thoughts", "add", "ses_06g30t7dv5qv55n1ppn3clis3k",
        "--name", "Review plan", "--operation-id", "op_06g30t8fudrq55fdkjqr6mpe44"]
stdin: Review the Unicode resize behavior.
```

Move or soft-delete a specified thought, then use persistent board undo if the
user asks to reverse it:

```console
proqi --json thoughts move ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 0
proqi --json thoughts delete ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388
proqi --json thoughts undo ses_06g30t7dv5qv55n1ppn3clis3k
proqi --json thoughts redo ses_06g30t7dv5qv55n1ppn3clis3k
```

Insert, move, delete, or duplicate separators and thoughts through the typed
mixed-item commands. Positions are zero-based in the shared `items` order, not
the thought-only projection. Supply each item once and in its current shared
Board order. Multi-item delete and duplicate are each one reversible Board
operation. Keep the returned `item_ids` and operation receipt.

```console
proqi --json items insert-separator ses_06g30t7dv5qv55n1ppn3clis3k --position 1
proqi --json items move ses_06g30t7dv5qv55n1ppn3clis3k sep_06g30t8fudrq55fdkjqr6mpe44 0
proqi --json items duplicate ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 sep_06g30t8fudrq55fdkjqr6mpe44
proqi --json items delete ses_06g30t7dv5qv55n1ppn3clis3k sep_06g30t8fudrq55fdkjqr6mpe44
```

Split, extract, merge, and reflow are exact Board transformations. Inspect each
source immediately beforehand and pass its returned digest. Byte offsets must
be UTF-8 boundaries. Extract ranges are start-inclusive and end-exclusive.
Merge requires at least two thoughts that are adjacent in the mixed Board
order, in the supplied order; an intervening separator makes them
noncontiguous. Supply one `--expected-sha256` for each merged thought. Merge
uses the installed, validated `merge_separator` setting, the same as the TUI.
While a TUI owns the session, its validated launch-time setting is authoritative
for both the initial request and exact retries. A later owner restart loads the
then-current setting, so changing it makes a merge with the same operation ID a
different semantic request.
Reflow uses Proqi's existing annotation-safe spacing policy and reports
`no_change` when the content is already clean or contains a layout that the
policy intentionally leaves exact, including whitespace-only content and
unsupported control characters.

```console
proqi --json thoughts split ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 8 --expected-sha256 <digest>
proqi --json thoughts extract ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 3 11 --expected-sha256 <digest>
proqi --json thoughts merge ses_06g30t7dv5qv55n1ppn3clis3k <first-tht-id> <second-tht-id> --expected-sha256 <first-digest> --expected-sha256 <second-digest>
proqi --json thoughts reflow ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 --expected-sha256 <digest>
```

Transformations preserve exact surviving identities, annotation ranges, and
mixed ordering through the canonical Board history. Split and extract retain
the source name on the source only. Merge retains the first thought's name.
Duplicate preserves each thought's name and annotation meaning while assigning
fresh attachment occurrence ordinals. Do not copy names into thought bodies.

Copy a specified thought into another user-specified session. Add `--remove`
only when the user explicitly asks to remove the source after destination
durability. Supply separate operation identifiers when retry safety matters:

```console
proqi --json thoughts send ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 destination-name --operation-id op_06g30t8fudrq55fdkjqr6mpe44
```

For remove-after-delivery, obtain a second canonical `op_` identifier for
`--remove-operation-id`. Never reuse one operation identifier for both steps.
If the command reports that destination delivery succeeded but source removal
failed, surface the structured receipt and do not retry with new identifiers
without the user's direction.

Use `--thought tht_06g30t8fudrq55fdkk348i7388` with undo or redo only when
the user explicitly requests that thought's editor history instead of board
history.

To obtain one canonical named session for automation, use `sessions ensure`.
It returns the live session with that exact name and origin directory or
creates it atomically, and it never opens a TUI or contacts Herdr. Report
`ambiguous_session` and `session_name_conflict` to the user instead of choosing.
Use `sessions create` only when the user explicitly wants an additional session
even if the name exists. Neither command opens the session; the user can run
the returned `resume_command`:

```console
proqi --json sessions ensure --name agent-os-claude --cwd /path/to/agent-os
proqi --json sessions create --name scratch --operation-id op_06g30t8fudrq55fdkjqr6mpe44
```

Session rename, trash, and restore use Browser history, which is separate from
every session's Board and editor history:

```console
proqi --json sessions rename ses_06g30t7dv5qv55n1ppn3clis3k "Unicode checks"
proqi --json sessions rename ses_06g30t7dv5qv55n1ppn3clis3k --clear
proqi --json sessions trash ses_06g30t7dv5qv55n1ppn3clis3k
proqi --json sessions restore ses_06g30t7dv5qv55n1ppn3clis3k
proqi --json sessions undo
proqi --json sessions redo
```

`sessions prune --yes` is permanent and is not Browser-undoable. Use it only
when the user explicitly requests permanent deletion of an already trashed
session.

Every session mutation except `sessions ensure` accepts `--operation-id`;
`ensure` is already idempotent by name and directory. Supply a fresh `op_`
identifier when a lost response must be retryable, and reuse it only to retry
the exact same request; a replay reports `idempotent_replay: true`. Trashing an
already trashed session succeeds with `changed: false`.

## Limitations

The JSON API intentionally does not expose spatial focus, cursor, selection,
viewport, resizing, hover, mouse dragging, modal state, clipboard access,
screen rendering, or arbitrary key injection. Host integration still owns
Herdr target discovery and submission. Do not simulate any of these by sending
TUI gestures. Use only the semantic commands advertised by the installed
capability manifest.

Single-thought collapse and expand are available through `thoughts collapse`.
Atomic multi-thought collapse or expand is not yet a public API operation; run
the advertised single-thought command separately for each explicit thought.
