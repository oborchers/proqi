# Unified undo and redo

This document defines the ownership, persistence, and truthfulness contract for
undo and redo. It is the design inventory for every semantic mutation and every
active input owner. The typed inventories in application and UI code must remain
exhaustive against this contract.

## Principles

1. Undo is contextual. The active editable owner receives undo and redo first.
2. A focused owner absorbs an unavailable undo or redo. It never falls through
   to hidden content.
3. One user intention is one undo unit unless the editor coalescing rules join
   adjacent intentions.
4. Durable local mutations survive restart. Transient text history lives only
   for the lifetime of its field.
5. Navigation, selection, focus, scrolling, hover, and temporary presentation do
   not create history.
6. External effects are never described as reversed. A paired local deletion can
   be reversed without recalling a clipboard write, prompt, destination copy,
   export, installed update, or external file change.
7. `UndoContract` is the compiler-exhaustive owner inventory for every
   normalized application mutation and every active shortcut context.
   Availability, labels, keyboard dispatch, pointer dispatch, Commands, Help,
   footer controls, owner control, and CLI receipts derive from those owners.

## History owners

### Local text owner

Search, Commands query, manual Invocation query, Transfer query, Global Delivery
query, Rename, Browser query, and Browser Rename each own an in-memory text
history. Each snapshot contains text, cursor head, optional selection anchor,
and the coalescing class. Closing the field destroys that history. Reopening the
field creates a new history, even when its initial text is the same.

Contiguous typing coalesces. Contiguous backward deletion and contiguous forward
deletion coalesce separately. Movement, selection changes, paste, replacement,
owner changes, and explicit boundaries close a group. Paste is one isolated
event. New content after undo clears the local redo branch. Selection-only
movement is not a normal undo event.

### Editor resource owner

Each durable thought owns Editor revisions. A revision stores exact before and
after content, annotations, cursor head, and optional selection anchor. Normal
typing uses deterministic intention and adjacency boundaries. Paste, invocation
insertion, indentation, sentence or logical-line deletion, and Edit Reflow are
single isolated units unless the initiating action explicitly composes them.

### Board owner

The current session owns Board operations. Board-only structural and presentation
mutations remain globally ordered. Operations that replace or redistribute
thought content declare every affected thought resource. Cross-owner undo and
redo compare the eligible Board operation with the active thought revision by
durable sequence. Atomic multi-thought operations move only when they are nearest
and applicable for every affected resource.

### Compose handoff owner

Compose is an ephemeral Editor resource until the first content-producing
intention. That intention atomically creates one durable thought and one durable
Board Create unit. The Create endpoints include exact content, annotations,
cursor head, and selection anchor. Undoing that Create restores empty Compose.
Redo restores the same thought identity, operation identity, content, annotation
state, cursor, and directional selection, including after restart.

### Browser administration owner

The session browser owns a durable cross-session administration history for
Rename, Trash, and Restore. It has its own cursor and sequence ordering and never
uses a selected session's Board or Editor cursor. Each operation stores a typed
inverse and the affected session identity. Trash retains enough identity and
metadata for undo to restore it without loading hidden thought history.

Browser history survives restart. Browser query and Browser Rename text owners
take precedence while alive, including after local undo empties the query. A
noneditable Browser overlay absorbs undo and redo as unavailable. CLI and
owner-control mutations use the same Browser operation owner when they perform
the same semantic action.

Permanent prune is not a history entry. It removes only operations and retained
idempotency receipts for the pruned session, reindexes the remaining global
Browser sequence, and adjusts its cursor. Unrelated Browser history remains
available. No later command can claim to restore the permanently removed data.

### Blocking owner

Recovery, Direction, Global Delivery disposition, Release Highlights, Update,
Screenshot, and Help are noneditable blocking owners. Undo and redo are handled
as unavailable while one is active. They never mutate the hidden owner.

## Mutation ownership matrix

| Semantic intention | Initiating route | Owner | Unit and inverse | External truth |
| --- | --- | --- | --- | --- |
| First Compose typing or paste | Compose keyboard or paste | Compose handoff through Board Create | One Create, undo to empty Compose | Local only |
| Create blank thought | Board keyboard, pointer, Commands | Board | One Create, inverse recoverable deletion | Local only |
| Paste as new thought | Board paste | Board | One Create containing the whole paste | Clipboard read is not reversed |
| Add thought | CLI, owner control, practice, screenshot capture | Board | One Create | Screenshot file is not removed |
| Typing and replacement | Edit | Editor resource | Coalesced revision | Local only |
| Paste into thought | Edit or Invocation | Editor resource | One isolated revision | Clipboard or source is not changed |
| Smart list, newline, indent, outdent | Edit | Editor resource | One revision per intention | Local only |
| Selection cut | Edit | Editor resource | One revision after clipboard success | Clipboard write stays |
| Sentence or logical-line deletion | Edit | Editor resource | One isolated revision | Local only |
| Exact content replacement | CLI or owner control | Editor resource | One revision | Local only |
| Edit Reflow | Edit | Editor resource | Pending typing first, then one Reflow revision | Local only |
| Delete one or many thoughts | Board, Commands, pointer, owner control | Board | One atomic recoverable deletion batch | Local only |
| Cut one or many thoughts | Board | Board | One deletion batch after clipboard success | Clipboard write stays |
| Duplicate thoughts | Board, Commands, pointer, owner control | Board | One atomic create batch | Local only |
| Reorder thoughts | Board keyboard, pointer, owner control | Board | One exact positional operation | Local only |
| Collapse or expand durably | Board keyboard, pointer, owner control | Board | One presentation operation | Local only |
| Split thought | Edit or Commands | Board plus affected thought resources | One atomic multi-thought operation | Local only |
| Extract selection | Edit or Commands | Board plus affected thought resources | One atomic multi-thought operation | Local only |
| Merge thoughts | Board or Commands | Board plus affected thought resources | One atomic multi-thought operation | Local only |
| Board Reflow | Board or Commands | Board plus affected thought resource | One content replacement operation | Local only |
| Submit and remove | Board, Edit, Commands | Board | Source deletion only after accepted delivery | Delivered prompt stays delivered |
| Transfer and remove | Transfer | Board | Source deletion only after destination receipt | Destination copy stays |
| Rename session | Board, Browser, CLI, owner control | Browser administration | One Rename with exact old and new names | Local only |
| Trash session | Browser or CLI | Browser administration | One Trash, inverse Restore | Local only |
| Restore session | Browser or CLI | Browser administration | One Restore, inverse Trash | Local only |
| Prune trashed sessions | CLI with confirmation | None | Irreversible | Removed data cannot be recalled |
| Copy | Board or Edit | None | No undo unit | Clipboard write stays |
| Submit and keep | Board, Edit, Commands | None | No undo unit | Delivered prompt stays delivered |
| Transfer and keep | Transfer | None | No undo unit | Destination copy stays |
| Export or external file mutation | CLI or integration | None | No undo unit | External files are not recalled |
| Update installation or restart | Update owner | None | No undo unit | Installation is not reversed |
| Navigation or selection movement | Any | None | No undo unit | Presentation only |
| Focus, scrolling, hover, modal navigation | Any | None | No undo unit | Presentation only |
| Temporary fold expansion | Edit | None | No undo unit | Presentation only |
| Search or query text mutation | Editable modal | Local text owner | In-memory field snapshot | Destroyed on close |
| Rename field text mutation | Rename modal | Local text owner | In-memory field snapshot | Destroyed on close |

## Active-context matrix

| Active context | Undo and redo target | Empty behavior |
| --- | --- | --- |
| Board with thought focus | Board | Quiet no-history state |
| Insertion boundary | Board | Quiet no-history state |
| Empty Compose | No target, except redo of an immediately undone Compose handoff | Absorb as unavailable when no handoff exists |
| Materialized Compose in Edit | Newest eligible active-thought Editor or Compose handoff unit | Quiet no-history state |
| Edit | Newest eligible active-thought Editor or resource-affecting Board unit | Quiet no-history state |
| Invocation over active Editor | Active Editor resource | Quiet no-history state |
| Search | Search local text history | Absorb locally |
| Commands | Commands query local text history | Absorb locally |
| Invocation query | Invocation query local text history | Absorb locally |
| Transfer query | Transfer query local text history | Absorb locally |
| Global Delivery query | Global Delivery query local text history | Absorb locally |
| Rename | Rename local text history | Absorb locally |
| Browser | Browser administration | Quiet no-history state |
| Browser query | Browser query local text history | Absorb locally |
| Browser Rename | Browser Rename local text history | Absorb locally |
| Recovery | Blocking owner | Absorb as unavailable |
| Direction | Blocking owner | Absorb as unavailable |
| Global Delivery disposition | Blocking owner | Absorb as unavailable |
| Release Highlights | Blocking owner | Absorb as unavailable |
| Update | Blocking owner | Absorb as unavailable |
| Screenshot | Blocking owner | Absorb as unavailable |
| Help | Blocking owner | Absorb as unavailable |

## Availability and feedback

The canonical resolver returns a concrete target and truthful semantic label, or
a typed unavailable reason. It never emits a generic invalid-state error for an
empty history. Commands, Help, footer controls, and pointer hit geometry consume
that same result. A hidden action is not assigned clickable geometry.

Successful feedback names only the local result. Examples include `Restored
source thought` after undoing submission removal and `Removed source thought`
after redoing it. It never claims to retract a delivered prompt, destination
copy, clipboard write, export, installed update, or external file mutation.

## Persistence and compatibility

Schema 15, storage protocol 14, and migration history rows 1 through 15 belong to
the Reflow contract and remain unchanged. The first unified-history migration is
16. Any new closed durable operation kind or payload meaning advances the storage
protocol to 15. Older revision payloads decode missing selection anchors as no
selection. Fresh databases record every migration row through 16. Upgrade,
backup authority, compaction, recovery, idempotent replay, busy retry, and
mixed-version refusal tests cover the new encodings.

## Researched editor precedents

The following sources were inspected as behavioral and architectural precedent.
No source code was copied.

| Project | Snapshot | License | Independently adopted ideas |
| --- | --- | --- | --- |
| Visual Studio Code | `abb16775f3ac861fdc281f8c1ef5d4e744f6c84a` | MIT | Focused editor or native input precedence, empty focused input absorption, resource stacks, atomic affected-resource operations |
| CodeMirror State | `9c801279cb83011e6f92af778f4443406e8f1200` | MIT | Directional anchor and head selection model, transaction-owned state |
| CodeMirror Commands | `5b9bac974f2c4af3e20b045adef949667872ecad` | MIT | Explicit history isolation, availability from depth, redo invalidation |
| CodeMirror View | `fbff59ba004d80d8c914f64c42586387b08706ac` | MIT | Paste as one input transaction |
| Helix | `079a789e8cb08ead67f19e1971a1b7438b37354b` | MPL 2.0 | Exact before and after selections, pending edit flush, precise oldest and newest feedback |

Proqi does not adopt Helix's branching history tree. Divergent content input
invalidates the relevant redo branch. Proqi also avoids a wall-clock-only typing
group boundary so tests and replay remain deterministic.
