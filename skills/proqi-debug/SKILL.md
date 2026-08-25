---
name: proqi-debug
description: Diagnose a reported Proqi failure using its content-redacted local diagnostics, stable CLI errors, and documented SQLite model. Use only when the user explicitly asks to debug Proqi or prepare a Proqi issue.
---

# Proqi Debug

Diagnose one reported failure safely. Begin read-only and minimize access to
scratchpad content.

## Workflow

1. Run `proqi --version`, `proqi --json capabilities`, and the read-only health
   report `proqi --json doctor`. Preserve every doctor check ID, status, and
   remediation exactly. Doctor never repairs, migrates, initializes, or
   recreates canonical Proqi state.
2. Record the installed
   version, CLI schema version, operating system, terminal, exact invocation,
   approximate failure time, and smallest reproduction.
3. Preserve the exact stable `error.code`, `error.message`, and `error.details`
   from JSON commands. Do not strengthen an intermittent symptom into a
   confirmed defect.
4. Ask before collecting local diagnostics. In explicitly authorized unattended
   QA mode, this approval is satisfied only for diagnostics produced from the
   run's isolated QA state. Never collect from the user's normal Proqi state in
   unattended mode. Use an explicit private output path that does not already
   exist:

   ```console
   proqi diagnostics collect --output proqi-diagnostics.json
   ```

5. Review the bundle locally before quoting or attaching it. It is designed to
   omit thought text, clipboard content, session names, workspace paths, pane
   identifiers, and raw external responses, but it retains typed identifiers
   and timestamps.
6. Correlate lifecycle events, command failures, and submission transitions by
   time and typed identifier. Never infer delivery failure merely from a later
   agent readiness state.
7. Reproduce with the smallest safe test session where practical. Do not read
   unrelated sessions or copy the user's live database merely for convenience.
8. Read [references/storage.md](references/storage.md) only when the failure
   involves persistence, migrations, leases, undo history, or submissions.
9. Classify the result as expected behavior, configuration or environment,
   unsupported integration, unconfirmed defect, confirmed defect, or security
   concern. State the evidence and any missing verification.

## Local files

Proqi uses the operating system's native application data directory. Diagnostic
segments are inside its `diagnostics` subdirectory, and the database is
`proqi.sqlite3` in the data directory.

- macOS: `~/Library/Application Support/proqi/`
- Linux: `${XDG_DATA_HOME:-~/.local/share}/proqi/`

Prefer `proqi diagnostics collect` over reading JSONL segments directly. Never
edit the live database, its `-wal` or `-shm` files, lock metadata, backups, or
diagnostic logs. Never run repair SQL against user data.

## Prepare an issue

For a confirmed non-security defect, first search existing Issues at
<https://github.com/oborchers/proqi/issues>. Then draft the exact issue title
and body in chat. Include:

- Proqi version and CLI schema version.
- Operating system and terminal.
- Minimal reproduction steps.
- Expected and observed behavior.
- Stable error code and a short content-redacted diagnostic excerpt.
- Whether the failure is reproducible and whether data was modified or lost.

Remove thought text, clipboard data, credentials, personal paths, session
names, pane identifiers, and identifiers that are not required to correlate the
failure. Do not attach the diagnostic bundle by default.

By default, obtain explicit approval of the complete issue text before running
`gh issue create`. Opening the issue is an external mutation. Report the issue
URL only after the command succeeds.

An unattended QA goal may provide advance authorization to create issues without
per-issue approval. Use that exception only when the goal explicitly names this
skill, restricts testing to an identified released binary, requires repeated
reproduction, defines a severity model, requires duplicate search, and authorizes
automatic GitHub issue creation. In that mode:

1. Create issues only for confirmed non-security defects reproduced at least
   twice on the identified released binary from isolated QA state.
2. Search open and closed issues first. Do not create a duplicate.
3. Use only existing repository labels. Do not create or edit labels, milestones,
   projects, repository settings, or releases.
4. Sanitize the complete title and body locally before creation.
5. Do not attach or upload diagnostics, logs, screenshots, databases, or other
   local evidence.
6. Create exactly one issue for each independently confirmed root defect, verify
   the created issue, and record its URL in the QA report.
7. If any prerequisite is uncertain, keep the finding local as unconfirmed or
   blocked instead of publishing it.

Never open a public issue for a suspected security vulnerability. Follow
`SECURITY.md` and use GitHub private vulnerability reporting instead. A private
security submission still requires explicit approval unless the unattended goal
separately and explicitly authorizes that exact channel.

## Boundaries

- Do not mutate SQLite or bypass Proqi's CLI, owner channel, or leases.
- Do not upload logs, bundles, databases, screenshots, or thought content.
- Do not retry a submission whose durable state is `outcome_unknown`.
- Do not claim a fix or successful delivery without direct evidence.
- Do not open, comment on, or modify a GitHub issue without explicit approval,
  except for new issues created within the narrow unattended QA authorization
  above.
