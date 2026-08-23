---
name: proqi
description: Inspect or update one user-specified Proqi scratchpad session through its stable JSON CLI. Use only when the user explicitly invokes Proqi or names this skill.
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
   merely because the skill is active. Never parse terminal escape sequences.
7. Perform only the requested mutation. Preserve operation receipts so the
   user can identify and reverse the change.

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

Use `--thought tht_06g30t8fudrq55fdkk348i7388` with undo or redo only when
the user explicitly requests that thought's editor history instead of board
history.
