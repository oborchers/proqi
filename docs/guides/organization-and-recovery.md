# Organize sessions and recover work

> Applies to Proqi 0.11.0.

Names and separators help you recognize work without changing prompt bodies.
Sessions keep separate boards resumable. Recovery operations distinguish
reversible local changes from permanent deletion.

## Name a thought without changing its body

Focus or edit a thought, then press `Ctrl+R` or choose **Rename thought** in
Commands. You can also click a visible thought name. Confirm with `Enter` or
cancel with `Escape`.

A name is optional, single-line organizational metadata. Creating a thought
never asks for one. Copying or delivering a thought uses its body and
annotations without prepending the name. Clearing a name does not clear the
body.

## Insert visual separators

Choose **Insert separator** in Commands. The new separator appears below the
focused Board item. On an empty Board it becomes the first item without
creating a blank thought.

A separator is a durable, payload-free Board item. It has no body, name,
attachment, or editor and does not own neighboring thoughts. Moving or deleting
it never moves or deletes those neighbors. Consecutive separators remain
distinct selectable items.

Separators participate in focus, selection, duplicate, delete, movement,
session persistence, and Board undo. Copy, cut, cleanup, collapse, and agent
delivery omit them. A recovery export retains their identity and Board order.

## Start, continue, and resume sessions

```sh
proqi                         # start a new session
proqi -c                      # continue the latest inactive session for this directory
proqi -r                      # open the searchable session browser
proqi -r <id-or-name>         # resume one exact session
proqi sessions list           # list resumable sessions
proqi sessions list -q text   # search names, paths, and thought content
```

A fresh launch starts a separate session so adjacent Proqi panes do not share a
board accidentally. `proqi -c` prefers the latest inactive session associated
with the current directory. Only one process may edit a given session at a
time, while different sessions can remain open concurrently.

In the session browser, type to filter and use arrows or a mouse click to choose
a resumable result. Active sessions remain visible but cannot be opened for
concurrent editing. `F2` renames and `F8` trashes the focused result only while
the search query is empty. Browser Undo and Redo cover rename, trash, and
restore across sessions.

## Know what is recoverable

| Operation | Recoverable? | Boundary |
| --- | --- | --- |
| Edit, create, delete, cut, duplicate, move, rename, separator change | Yes | Persistent Editor or Board undo |
| Session rename, trash, restore | Yes | Persistent Browser undo |
| Accepted submit with local removal | Local removal only | Undo restores the source but cannot recall the delivery |
| Cross-session send and remove | Source removal only | Undo does not retract the destination copy |
| Clipboard write | No external recall | Local cut removal is still undoable |
| Session prune | No | Requires a previously trashed session and `--yes` |

Use Commands for contextual Undo and Redo without memorizing a chord. On macOS,
`Ctrl+Z`, `Ctrl+Shift+Z`, and `Ctrl+Y` are terminal-safe defaults. Board `u` and
the conventional Primary aliases remain available when the terminal forwards
them. See the canonical [undo and redo guide](../UNDO_REDO.md) for every active
owner.

## Trash, restore, and prune safely

```sh
proqi sessions trash <id-or-name>
proqi sessions restore <id-or-name>
proqi sessions undo
proqi sessions redo
proqi sessions prune <id-or-name> --yes
```

Trash is recoverable. Restore returns the session. Prune is permanent and only
accepts an already trashed session after the explicit confirmation flag. Review
the exact target with `proqi sessions list --all` before pruning.

## Recover from a storage failure

There is no save command. The footer distinguishes pending work from durable
work. If storage fails, Proqi keeps the optimistic in-memory board visible and
does not report it as saved.

The Recovery surface offers:

- `r` to retry the failed storage operation;
- `w` to write a private recovery export;
- `q` or `Primary+Q` to request exit through the same durability checks;
- `Escape` to close the overlay without discarding the failure.

Do not edit the database, delete lock files, or start a second writer to force
recovery. If retry remains unavailable, export first and use the exact resume
command and recovery path that Proqi reports. Recovery format 2 retains thought
and separator identities, ordering, timestamps, and recoverable deletion state.

For a read-only health check that does not repair or mutate state:

```sh
proqi doctor
```

To create a local, content-redacted support bundle at a path you choose:

```sh
proqi diagnostics collect --output proqi-diagnostics.json
```

Proqi refuses to overwrite an existing bundle and never uploads it. Review the
file before sharing it.

Normal exit prints the exact resume command. A crash or terminal closure leaves
committed content resumable. On macOS and Linux, a confirmed input-reader stall
may trigger one bounded same-pane replacement attempt for the exact session.
If Proqi cannot prove safe replacement, it restores the terminal and exits with
manual recovery instructions instead of looping or guessing.
