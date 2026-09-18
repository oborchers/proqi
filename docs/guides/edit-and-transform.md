# Transform and organize thoughts

<span class="version-scope">Proqi 0.11.0</span>

Proqi can reshape prompt material without turning the board into a document
editor. Every transformation is one durable operation with an explicit local
undo path.

## Split at the cursor

While editing with no text selection, use `Primary+T` or choose **Split
thought at cursor** in Commands. The content before and after the exact cursor
becomes two ordered thoughts.

The transformation keeps the original thought identity on its defined side and
creates one new thought for the other side. It does not reinterpret whitespace
or Markdown.

## Extract selected text

Select text in Edit, then use `Primary+T` or choose **Extract selection as new
thought**. The exact selection becomes a neighboring thought and is removed
from the source as part of the same atomic operation.

## Merge a contiguous range

Build a contiguous Board range, then press the configured transform key,
factory `t`, or choose **Merge selected thoughts**. Proqi joins thought bodies
with the configured `merge_separator`, which defaults to one blank line.

Separators are structural items and never become prompt payload. Merge requires
the eligible contiguous thought selection shown by Commands.

## Duplicate, move, and collapse

- **Duplicate thought or selection** creates copies below the source range and
  selects the copies. Separators duplicate as structural items.
- **Move item up** and **Move item down**, keyboard reorder, and dragging move
  only the focused item. Selected-block reorder is intentionally unavailable.
- **Expand or collapse thought** changes durable presentation without changing
  text. In a mixed selection, separators are ignored.

Keyboard reorder wraps at board edges. Dragging is positional and does not
wrap.

## Clean existing spacing

Focus a thought and press `f`, or choose **Clean up spacing**. In Edit,
`Ctrl+Shift+F` cleans the complete thought rather than only the text
selection. Protected code, tables, quotes, paths, URLs, controls, list
structure, and attachment annotations stay exact.

An unchanged result creates no history entry. See
[Paste and attachments](paste-and-attachments.md) for the full cleanup rules.

## Add visual structure without payload

Choose **Insert separator** in Commands. A separator is a persistent Board item
that can be focused, selected, moved, dragged, duplicated, deleted, undone, and
redone. It has no body, cannot be named, and never enters copied, merged, or
delivered text.

Thought and session names are also organizational metadata. They remain
separate from prompt bodies. See
[Sessions and recovery](organization-and-recovery.md).

## Undo transformations safely

Split, extract, and merge affect Board structure and one or more editor
resources as one atomic operation. Newer edits to an affected thought can make
the Board operation temporarily unavailable until that thought's editor history
moves first. Proqi reports the active owner instead of undoing hidden content.

See [Undo and redo](../reference/undo-redo.md).
