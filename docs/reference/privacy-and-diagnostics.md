# Privacy, durability, and diagnostics

<span class="version-scope">Proqi 0.11.0</span>

Proqi stores thoughts, annotations, attachments created from clipboard pixels,
settings, and redacted logs locally. It has no telemetry, cloud sync,
collaboration service, or content upload.

## Know what can leave the machine

| Action | External effect |
| --- | --- |
| Copy | Writes exact body text to the native clipboard or bounded OSC 52 fallback. |
| Direct delivery | Sends assembled prompt text to one verified Herdr agent target. |
| Cross-session send | Copies one thought into another local Proqi session. |
| Update check | Makes a content-free request to the verified release channel. |
| Attachment path | The path stays in prompt content; the receiving agent may read it after delivery. |
| Diagnostics collect | Writes a new redacted local file only. Nothing is uploaded. |
| Recovery export | Writes a new private local recovery file only. |

Proqi never reads an agent conversation, waits for its response, injects raw
keys as delivery fallback, or requests an iCloud attachment download.

## Read durability truthfully

The footer distinguishes pending, durable, and failed persistence. Destructive
quit is blocked while failure handling requires a choice. Retry attempts the
failed save. Recovery export preserves visible optimistic state without
claiming that the main store contains it.

Acknowledged changes survive forced termination. Uncommitted SQLite writes roll
back. One authoritative lease prevents two processes from silently editing the
same session.

## Run read-only health checks

```sh
proqi doctor
```

Doctor inspects local health without repairing, moving, deleting, or uploading
user data.

## Collect a private support bundle

```sh
proqi diagnostics collect --output proqi-diagnostics.json
```

The destination must be new; existing files are never overwritten. Diagnostics
are bounded and content-redacted, but review the file before sharing it.

Update lifecycle diagnostics contain closed stages, aggregate counts, stable
failure codes, and convergence outcomes. Input-recovery diagnostics contain a
stable stage, reason, attempt count, and outcome. They omit thought content,
session identity, local paths, pane identity, and raw terminal bytes.

## Inspect one key safely

```sh
proqi diagnostics keypress --context board,edit --timeout-ms 5000
proqi --json diagnostics keypress --context board --defaults
```

The diagnostic captures one logical key event and restores terminal state on
normal completion, cancellation, timeout, and error. It records no paste or
session content. `--defaults` inspects factory behavior without loading user
configuration.

See [Shortcut troubleshooting](../guides/shortcuts.md).
