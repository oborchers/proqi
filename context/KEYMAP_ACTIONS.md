# Keymap action inventory

Schema version 1 uses these stable identifiers. A context listed here permits
a binding, subject to text safety, collisions and required recovery routes.
An eligible action may still be unavailable in the current application state.
Commands availability and execution use the same action descriptor.

The mechanical inventory test checks every effective binding, descriptor,
presentation reference and diagnostic identity for both platform policies.
The foundation parity fixture pins every default event and intention with
explicit version 1 migration changes.

The user-facing factory shortcut tables are in the
[README](../README.md#board-controls). Runtime Help and footer labels are
projected from the resolved registry, so configured aliases replace those
factory labels without requiring another presentation table.

| Action | Eligible contexts | Safety | Commands |
| --- | --- | --- | --- |
| `thought.new` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `session.rename` | `board`, `compose`, `edit`, `commands`, `invocation`, `browser`, `insertion_boundary` | Ordinary | yes |
| `session.copy_id` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `session.copy_resume` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `session.send` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `session.send_remove` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.edit` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.plain_newline` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.jump_up` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.jump_down` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.select_visual_row_start` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.select_visual_row_end` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.thought_start` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.thought_end` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.indent` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.outdent` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.split` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.extract_selection` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.merge` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.delete` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | DestructiveUndoable | yes |
| `submission.submit_remove` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `submission.submit_to_agent` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `submission.submit_all_remove` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `submission.submit_all_keep` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `agents.refresh` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `attachments.refresh` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `invocation.insert` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `invocation.refresh` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `update.check` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `update.whats_new` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `screenshot.inbox` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `screenshot.retry_capture` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `recovery.retry_storage` | `board`, `compose`, `edit`, `commands`, `invocation`, `recovery`, `insertion_boundary` | RecoveryCritical | yes |
| `recovery.export` | `board`, `compose`, `edit`, `commands`, `invocation`, `recovery`, `insertion_boundary` | RecoveryCritical | yes |
| `thought.move_up` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.move_down` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.collapse` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.select` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.range_select` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `help.open` | `board`, `compose`, `edit`, `commands`, `invocation`, `recovery`, `insertion_boundary` | Ordinary | yes |
| `application.quit` | `board`, `compose`, `edit`, `help`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `global_delivery_disposition`, `browser`, `browser_query`, `rename`, `browser_rename`, `update`, `screenshot`, `recovery`, `direction`, `release_highlights`, `insertion_boundary` | RecoveryCritical | yes |
| `context.close` | `board`, `compose`, `edit`, `help`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `global_delivery_disposition`, `browser`, `browser_query`, `rename`, `browser_rename`, `update`, `screenshot`, `recovery`, `direction`, `release_highlights`, `insertion_boundary` | InvariantClose |  |
| `context.confirm` | `compose`, `edit`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `global_delivery_disposition`, `browser`, `browser_query`, `rename`, `browser_rename`, `update`, `screenshot`, `direction` | Ordinary |  |
| `text.backspace` | `compose`, `edit`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `browser`, `browser_query`, `rename`, `browser_rename` | TextEditing |  |
| `text.delete_forward` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | TextEditing |  |
| `text.tab` | `compose`, `edit` | Ordinary |  |
| `text.backtab` | `compose`, `edit`, `invocation` | Ordinary |  |
| `list.previous` | `board`, `help`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `global_delivery_disposition`, `browser`, `browser_query`, `update`, `screenshot`, `release_highlights`, `insertion_boundary` | Ordinary |  |
| `list.next` | `board`, `help`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `global_delivery_disposition`, `browser`, `browser_query`, `update`, `screenshot`, `release_highlights`, `insertion_boundary` | Ordinary |  |
| `board.range_previous` | `board`, `insertion_boundary` | Ordinary |  |
| `board.range_next` | `board`, `insertion_boundary` | Ordinary |  |
| `navigation.fast_previous` | `board`, `compose`, `edit`, `help`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `global_delivery_disposition`, `browser`, `browser_query`, `rename`, `browser_rename`, `update`, `screenshot`, `recovery`, `direction`, `release_highlights`, `insertion_boundary` | Ordinary |  |
| `navigation.fast_next` | `board`, `compose`, `edit`, `help`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `global_delivery_disposition`, `browser`, `browser_query`, `rename`, `browser_rename`, `update`, `screenshot`, `recovery`, `direction`, `release_highlights`, `insertion_boundary` | Ordinary |  |
| `navigation.fast_extend_previous` | `board`, `compose`, `edit`, `help`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `global_delivery_disposition`, `browser`, `browser_query`, `rename`, `browser_rename`, `update`, `screenshot`, `recovery`, `direction`, `release_highlights`, `insertion_boundary` | Ordinary |  |
| `navigation.fast_extend_next` | `board`, `compose`, `edit`, `help`, `commands`, `search`, `invocation`, `invocation_query`, `transfer`, `global_delivery_query`, `global_delivery_disposition`, `browser`, `browser_query`, `rename`, `browser_rename`, `update`, `screenshot`, `recovery`, `direction`, `release_highlights`, `insertion_boundary` | Ordinary |  |
| `board.first_thought` | `board`, `commands`, `insertion_boundary` | Ordinary | yes |
| `board.last_thought` | `board`, `commands`, `insertion_boundary` | Ordinary | yes |
| `board.range_first_thought` | `board`, `insertion_boundary` | Ordinary |  |
| `board.range_last_thought` | `board`, `insertion_boundary` | Ordinary |  |
| `editor.grapheme_back` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.grapheme_forward` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.word_back` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.word_forward` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.document_start` | `compose`, `edit` | Ordinary |  |
| `editor.document_end` | `compose`, `edit` | Ordinary |  |
| `editor.visual_up` | `compose`, `edit` | Ordinary |  |
| `editor.visual_down` | `compose`, `edit` | Ordinary |  |
| `editor.line_start` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.line_end` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.extend_grapheme_back` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.extend_grapheme_forward` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.extend_word_back` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.extend_word_forward` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.extend_visual_up` | `compose`, `edit` | Ordinary |  |
| `editor.extend_visual_down` | `compose`, `edit` | Ordinary |  |
| `editor.extend_document_start` | `compose`, `edit` | Ordinary |  |
| `editor.extend_document_end` | `compose`, `edit` | Ordinary |  |
| `editor.extend_line_start` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.extend_line_end` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.extend_visual_row_start` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.extend_visual_row_end` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.move_visual_row_start` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `editor.move_visual_row_end` | `compose`, `edit`, `commands`, `search`, `invocation`, `transfer`, `global_delivery_query` | Ordinary |  |
| `clipboard.copy` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `clipboard.cut` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | DestructiveUndoable | yes |
| `clipboard.paste_exact` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.reflow` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `clipboard.paste_reflow` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `thought.insert_above` | `board`, `commands`, `insertion_boundary` | Ordinary | yes |
| `thought.insert_below` | `board`, `commands`, `insertion_boundary` | Ordinary | yes |
| `selection.select_all` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `board.duplicate` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `history.undo` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `history.redo` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `submission.submit_keep` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | Ordinary | yes |
| `editor.delete_logical_line` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | TextEditing | yes |
| `editor.delete_sentence` | `board`, `compose`, `edit`, `commands`, `invocation`, `insertion_boundary` | TextEditing | yes |
| `invocation.previous` | `invocation`, `invocation_query` | Ordinary |  |
| `invocation.next` | `invocation`, `invocation_query` | Ordinary |  |
| `thought.contextual_transform` | `board`, `compose`, `edit`, `invocation`, `insertion_boundary` | Ordinary |  |
| `search.open` | `board`, `compose`, `edit`, `invocation`, `insertion_boundary` | Ordinary |  |
| `commands.open` | `board`, `compose`, `edit`, `invocation`, `insertion_boundary` | Ordinary |  |
| `browser.trash` | `browser` | Ordinary |  |
| `direction.left` | `direction` | Ordinary |  |
| `direction.down` | `direction` | Ordinary |  |
| `direction.up` | `direction` | Ordinary |  |
| `direction.right` | `direction` | Ordinary |  |
