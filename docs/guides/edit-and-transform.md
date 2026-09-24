# Transform and organize thoughts

<span class="version-scope">Proqi 0.11.0</span>

Proqi can reshape prompt material without turning the board into a document
editor. Every transformation is one durable operation with an explicit local
undo path.

## Split at the cursor

While editing with no text selection, use `Primary+T` or choose **Split
thought at cursor** in Commands. The content before and after the exact cursor
becomes two ordered thoughts.

The original thought keeps its identity, name, and exact left content. The
untrimmed right content becomes a new thought immediately below, receives focus
in Edit, and places its cursor at the start. Either half may be empty. Split
does not reinterpret whitespace or Markdown.

## Extract selected text

Select text in Edit, then use `Primary+T` or choose **Extract selection as new
thought**. The exact nonempty selection becomes a new thought immediately below
and is removed from the source as part of the same atomic operation. The new
thought receives focus in Edit with its cursor at the end. An empty or stale
selection is rejected without changing either thought.

## Merge a contiguous range

Build a contiguous Board range, then press the configured transform key,
factory `t`, or choose **Merge selected thoughts**. Proqi joins thought bodies
with the configured `merge_separator`, which defaults to one blank line.

Separators are structural items and never become prompt payload. In v0.11.0,
merge requires at least two adjacent live thoughts; a separator between those
thoughts is not merged and remains on the Board. The first thought keeps its
identity and name, the others are recoverably deleted, the survivor receives
Board focus, and selection clears.

On next-release `main`, thought contiguity follows the mixed Board order. An
intervening separator therefore makes the merge fail without changing the
Board.

Split, extract, and merge preserve an annotation whose complete semantic text
survives. A boundary-crossing attachment, fold, invocation reference, or
shortcut-emphasis range dissolves rather than describing only a fragment.

## Duplicate, move, and collapse

- **Duplicate thought or selection** creates copies below the source range and
  selects the copies. Separators duplicate as structural items.
- **Move item up** and **Move item down** exchange each selected Board run with
  one adjacent unselected item. Selected runs at an edge stay put. With one
  focused item, keyboard reorder and dragging keep their existing behavior.
- **Expand or collapse thought** changes durable presentation without changing
  text. In a mixed selection, separators are ignored.

Single-item keyboard reorder wraps at board edges. Dragging is positional and does not
wrap.

## Clean existing spacing

Select thoughts and press `f`, or choose **Clean up spacing**. In Board, each
eligible selected thought is cleaned in one durable undo step; separators
remain untouched. With no selection, the focused thought is cleaned. In Edit,
`Ctrl+Shift+F` cleans the complete thought rather than only the text
selection. Protected code, tables, quotes, paths, URLs, controls, list
structure, and attachment annotations stay exact.

An unchanged result creates no history entry. See
[Paste and attachments](paste-and-attachments.md) for the full cleanup rules.

## Add visual structure without payload

Choose **Insert separator** in Commands. A separator is a persistent Board item
that can be focused, selected, moved, dragged, duplicated, deleted, undone, and
redone. It has no body, cannot be named, and never enters copied or delivered
text. In v0.11.0 it remains in place when the adjacent live thoughts around it
are merged. On next-release `main`, it prevents that merge.

Thought and session names are also organizational metadata. They remain
separate from prompt bodies. See
[Sessions and recovery](organization-and-recovery.md).

## Undo transformations safely

Split, extract, and merge affect Board structure and one or more editor
resources as one atomic operation. Newer edits to an affected thought can make
the Board operation temporarily unavailable until that thought's editor history
moves first. Proqi reports the active owner instead of undoing hidden content.

See [Undo and redo](../reference/undo-redo.md).

## Apply the same operations from the CLI

<span class="version-scope">Next release</span>

The following CLI operations are available on `main` for the next release, not
in the published v0.11.0 binary.

The JSON CLI exposes these exact Board operations for scripts and coding
agents. Inspect the thought first, retain its `content_sha256`, then submit that
digest as the content precondition:

```text
proqi --json thoughts split <session> <thought> <at-byte> --expected-sha256 <sha256>
proqi --json thoughts extract <session> <thought> <start-byte> <end-byte> --expected-sha256 <sha256>
proqi --json thoughts merge <session> <first> <second> --expected-sha256 <first-sha256> --expected-sha256 <second-sha256>
proqi --json thoughts reflow <session> <thought> --expected-sha256 <sha256>
```

Offsets are UTF-8 byte positions, not displayed columns or perceived-character
counts. A stale digest, invalid character boundary, empty extraction, or merge
across a separator fails without a partial result. Use a fresh
`--operation-id` when a mutation may be retried after an uncertain caller-side
failure.

The `items` family inserts, moves, duplicates, or recoverably deletes thoughts
and payload-free separators in shared Board order. See the canonical
[command-line reference](../reference/cli.md#change-board-items) for every
argument and retry rule.
