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
3. Address the session the user specified. If none is unambiguous, run
   `proqi --json sessions list` or add `--query <text>`, show the matches, and
   let the user choose. Never guess from recency alone.
4. Run all further commands with `--json`. On `ok: false`, surface
   `error.code`, `error.message`, and `error.details` without rewriting them.
   Do not automatically retry busy, ambiguous, conflicting, or unsupported
   mutations.
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
session, `tht_` a thought, `op_` a board operation, `rev_` an editor revision,
`req_` a control request, `sub_` a submission receipt, and `ins_` a running
instance. The suffix is 26 lowercase base32hex characters containing a complete
UUIDv7. Never shorten, re-case, fabricate, or use one prefix where another type
is expected.

The examples below use these canonical fixtures:

```text
ses_06g30t7dv5qv55n1ppn3clis3k
tht_06g30t8fudrq55fdkk348i7388
op_06g30t8fudrq55fdkjqr6mpe44
```

Replace them only with same-typed identifiers returned by the live CLI.

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

List and inspect synchronize with an active session owner before reading. Both
return `content_sha256`. Use that digest as the precondition for exact
replacement so a concurrent human edit cannot be overwritten:

```text
argv:  ["proqi", "--json", "thoughts", "replace",
        "ses_06g30t7dv5qv55n1ppn3clis3k",
        "tht_06g30t8fudrq55fdkk348i7388",
        "--expected-sha256", "<digest returned by inspect>"]
stdin: Exact replacement content.
```

Use `--force` only when the user explicitly asks to replace the current content
regardless of intervening edits. It never bypasses a submission lock. Exact
replacement is an editor revision and can be reversed with thought-scoped undo.

Set durable collapsed presentation explicitly when requested:

```console
proqi --json thoughts collapse ses_06g30t7dv5qv55n1ppn3clis3k tht_06g30t8fudrq55fdkk348i7388 --collapsed true
```

Add exact content by direct process execution, with the prompt as standard
input rather than an argument:

```text
argv:  ["proqi", "--json", "thoughts", "add", "ses_06g30t7dv5qv55n1ppn3clis3k"]
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
