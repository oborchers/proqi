# Capture and edit thoughts

<span class="version-scope">Proqi 0.11.0</span>

A thought is one independently editable prompt body. Proqi preserves its exact
text, including Unicode, whitespace, tabs, and line endings. An optional name
can help organize it, but that name is never prepended to copied or delivered
content.

## Create where you are

| Route | Result |
| --- | --- |
| Press `n` | Create a blank thought and enter Edit |
| Activate **+ New thought** | Create at the insertion row |
| Paste on the Board | Create one thought containing the complete paste |
| Type on an empty board | Materialize the first thought from Compose |
| Choose **New thought** in Commands | Create and edit a blank thought |
| Choose **Insert thought above** or **below** | Create beside the focused item |

At a blocked top or bottom boundary, repeat the same vertical navigation
intention to create beyond the edge. An explicitly created blank thought is
durable. An untouched empty Compose surface is not.
An item added through the API to an active empty board becomes the focused Board
item after it is saved. This does not open its text editor. Further API additions
leave the current focus in place.

## Edit exact text

Press `Enter` or `e`, or click a thought body. Cursor movement operates on
grapheme clusters, so combining marks, emoji sequences, CJK, wide characters,
tabs, and CRLF content remain intact.

Common editor actions include:

- move or extend by grapheme, word, visual row, logical line, or complete
  thought;
- jump five rendered rows with `Alt+Up` and `Alt+Down` or page keys;
- select to the current wrapped row edge on macOS with Command plus a horizontal
  arrow, adding Shift to extend;
- insert a literal newline through **Insert plain newline** when smart-list
  continuation is not wanted;
- delete the logical line with `Primary+U`;
- delete every Unicode sentence touched by the cursor or selection with
  `Primary+Shift+U`;
- indent or outdent a recognized list item or selected list range.

Sentence deletion uses a defined Unicode boundary profile, not linguistic
inference. Abbreviations, source code, and punctuation-only text can produce
surprising boundaries. If a target intersects folded content, the first command
reveals every touched fold without deleting; review it, then repeat deliberately.
Undo is the safe response when the resulting deletion was not intended.

## Continue lists without losing plain text

With `smart_lists = true`, the factory default, `Enter` continues recognized
unordered, ordered, and task-list markers. `Tab` nests a recognized list or
inserts configured spaces in ordinary text. `Shift+Tab` outdents recognized
list structure and leaves unrelated text unchanged.

Set `smart_lists = false` to make ordinary Enter insert an unstructured newline.
The **Insert plain newline** command is always an explicit alternative.

## Work with long thoughts

Press `c` or choose **Expand or collapse thought**. Collapse changes only the
presentation and is durable; canonical content stays exact. Large pastes and
attachment paths also use compact folded annotations with atomic cursor and
selection behavior.

Scrolling advances by rendered rows and keeps focus and the insertion row
reachable. Resizing reflows the presentation without changing content, logical
cursor position, selection, or durability.

## Search without changing the board

Press `/` in Board mode. Search covers thought content and keeps the board
unchanged. Session Browser search additionally covers session names and launch
paths. Closing a search field discards its transient query history, not board
history.

## Next steps

- [Paste, clean up, and attach files](paste-and-attachments.md)
- [Transform and organize](edit-and-transform.md)
- [Undo and redo](../reference/undo-redo.md)
