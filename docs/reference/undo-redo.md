# Undo and redo

<span class="version-scope">Proqi 0.11.0</span>

Undo is contextual. It acts on the owner you can currently see and edit, never
on hidden content behind an overlay. Durable Board and editor history survives
restart; temporary query-field history ends when that field closes.

## What each context owns

| Active context | Undo and redo target |
| --- | --- |
| Board or insertion row | Persistent Board operations |
| Edit or inline invocation | Newest eligible revision for the active thought, including affected Board transformations |
| Search, Commands, transfer, delivery, or rename query | That temporary text field only |
| Session Browser | Persistent rename, trash, and restore history |
| Recovery, direction, update, screenshot, highlights, or Help | Absorbed as unavailable; hidden history is untouched |

If one transformation affects several thoughts, a newer edit to any affected
thought may need to be undone first. Proqi reports the blocking owner instead of
silently changing another resource.

## Persistent local operations

One undo unit covers each deliberate operation:

- create, delete, cut, duplicate, reorder, collapse, rename, or reflow;
- split, extract, or merge;
- one exact paste, invocation insertion, indentation, sentence deletion, or
  logical-line deletion;
- accepted delivery or transfer removal;
- Session Browser rename, trash, or restore.

Ordinary adjacent typing and same-direction deletion can coalesce. Navigation,
selection, focus, scrolling, hover, and temporary fold expansion create no
history entry.

## External effects are not recalled

Undo can restore local source thoughts removed after delivery, transfer, or cut.
It cannot retract the already accepted prompt, destination copy, clipboard
write, exported file, installed update, or external attachment change.

**Submit and keep**, **Send and keep**, and Copy do not create local undo units
because they do not change the source Board.

## Irreversible operation

`proqi sessions prune <session> --yes` permanently deletes an already trashed
session. Prune is never entered into history. Trash first when recovery may
still be needed.

The contributor-level exhaustive ownership contract remains in
[docs/UNDO_REDO.md](https://github.com/oborchers/proqi/blob/0b016f767af747f013301aaf41bcd0bf1cf827ef/docs/UNDO_REDO.md).
