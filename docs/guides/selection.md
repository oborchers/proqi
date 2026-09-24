# Select and act on thoughts

> Applies to Proqi 0.11.0.

Proqi always has one focused Board item or the final insertion row. A selection
is a separate group of thoughts and separators. Moving focus does not silently
change an existing arbitrary selection.

## Focus one item

Use `j` and `k`, or the arrow keys, to move focus. A mouse click focuses the
item under the pointer. The focused item has the strong gutter treatment and is
the target for editing, naming, reordering without a selection, and any action used
without a selection.

`Ctrl+Down` and `Ctrl+Up`, or `Ctrl+j` and `Ctrl+k`, jump to the last and first
live thoughts. `Page Down` and `Page Up` move five thoughts at a time. These
movements stop at real thoughts and do not include the final insertion row in a
selection.

Press `Enter`, press `e`, or click body text to edit the focused thought.
Entering Edit clears the Board selection because text selection is then owned
by that thought's editor.

## Toggle an arbitrary group

Press `Space` to select or deselect the focused item. Move focus and repeat to
build a noncontiguous group. Use the visible selection control for the same
mouse workflow.

Press `a` or `Primary+A` in Board mode to select every live Board item. In Edit,
`Primary+A` selects only the active thought's text, while plain `a` remains
ordinary text.

Press `Escape` in Board mode to clear the complete Board selection.

## Select a contiguous range

Use any of these routes:

- Hold `Shift` while pressing an arrow.
- Press uppercase `K` or `J`.
- Hold `Shift` while clicking the range endpoint.
- Press `v` to latch range selection, then use arrows, `j`, `k`, or a click.
- Use `Shift+Page Up` or `Shift+Page Down` to extend by five items.

The first range action establishes an anchor and replaces any arbitrary
`Space` selection. Reversing direction shrinks the range, then extends past the
same anchor. Pressing `Space` drops the anchor, keeps the already selected items
as an arbitrary selection, and toggles the focused item.

The `v` latch is useful when a terminal does not forward Shift reliably. A
modal overlay or Edit mode releases the latch. `Escape` clears both the range
and the latch.

## What group actions include

Actions process the selection in visible Board order.

| Action | Thoughts | Separators | Important result |
| --- | --- | --- | --- |
| Copy | Included | Omitted | Bodies are joined with one blank line. Names are not copied. |
| Cut | Included | Omitted and left on the Board | Clipboard verification must succeed before thoughts are removed. |
| Delete | Included | Included | One recoverable Board operation and one undo step. |
| Duplicate | Included | Included | Copies appear below the source range and become selected. |
| Collapse or expand | Included | Ignored | Content does not change. |
| Move up or down | Included | Included | Each selected run exchanges with one adjacent unselected item. Edge runs stay put. |
| Clean up spacing | Included | Ignored | Changed thoughts form one undoable Board operation; an unchanged result creates no history. |
| Deliver to agent | Included | Omitted | One prompt request contains the ordered bodies. |
| Send to another Proqi session | Included | Omitted and left on the Board | Destination copies commit as one cohort in selected order. |
| Send and remove | Included | Omitted and left on the Board | Sources are removed together only after the complete destination cohort is durable. |

A separator-only copy, cut, or delivery is a visible no-op. It does not erase
the clipboard and does not send an empty prompt.

Annotated copy and cut require generation-bound typed clipboard support, which
is available on macOS. Other supported platforms reject those operations
rather than silently losing folds, attachment, or invocation metadata.

Names remain organizational metadata. Copying or delivering a thought uses its
body and annotations without prepending the name.

## Reorder selected items

Use `Primary+Shift+Up` and `Primary+Shift+Down`, `Primary+K` and `Primary+J`,
or the drag handle. On macOS, `Option+Shift+Up` and `Option+Shift+Down` are
additional factory aliases. With multiple selected items, each contiguous run
exchanges with the adjacent unselected item in the requested direction.
Disjoint runs keep their internal order; an edge run stays put. Selection and
focus retain their exact item identities. An effective group move is one
durable undo step.

Single-item keyboard reordering wraps at Board edges. Dragging is positional and does not
wrap. Each move is saved and undoable.

## Undo a group action

A bulk delete, cut, duplicate, collapse, selected reorder, selected spacing
cleanup, or accepted remove-after-delivery is
one Board operation. One Undo restores the complete local operation, even after
a restart. Undoing local removal does not recall a clipboard write or a prompt
already accepted by an external agent.

See [Undo and redo](../reference/undo-redo.md) for active-owner rules
and [Deliver prompts to agents](agent-delivery.md) for receipt and removal
semantics.
