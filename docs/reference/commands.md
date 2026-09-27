# Commands palette reference

<span class="version-scope">62 actions in the next release (59 in Proqi 0.14.0)</span>

Press `:` in Board mode to open Commands. Search, use the arrow keys or pointer
to choose a result, then press `Enter` or click. The same canonical
registry owns availability, execution, effective shortcut display, and
diagnostic identity.

An action appears only when its prerequisites are true. **Relevant now** is a
short contextual view, not the complete inventory. Typing a query searches all
available actions below.

## Thoughts and Board

| Command | What it does | Important condition or result |
| --- | --- | --- |
| **New thought** | Creates a blank thought and enters Edit. | Requires a writable Board. |
| **Insert thought above** | Creates a thought above the focused item. | Requires a focused Board item. |
| **Insert thought below** | Creates a thought below the focused item. | Requires a focused Board item. |
| **Insert separator** | Adds a persistent visual separator below focus. | Separator is payload-free. |
| **Edit thought** | Opens the focused thought in Edit. | Separators cannot be edited. |
| **Rename thought** | Sets or clears the optional organizational name. | Name never enters the body. |
| **Delete item or selection** | Soft-deletes the focused item or selected group. | One recoverable Board operation. |
| **Copy thought text** | Copies ordered thought bodies. | Names and separators are omitted. |
| **Cut thought text** | Copies, then removes thoughts after clipboard success. | Separators remain on the Board. |
| **Paste exactly** | Inserts clipboard content without cleanup. | Creates a thought on Board or edits at the cursor. |
| **Paste and clean up** | Applies explicit safe spacing cleanup while pasting. | Protected structures stay exact. |
| **Clean up spacing** | Cleans eligible selected thoughts in Board, or the focused or active thought. | Separators stay untouched. An effective Board selection is one history entry; a no-op creates none. |
| **Duplicate thought or selection** | Duplicates focused or selected items below the source. | Duplicated results become selected. |
| **Move item up** | Moves selected Board items up one unselected neighbor, or the focused item. | Selected edge runs stay put; a single item wraps. |
| **Move item down** | Moves selected Board items down one unselected neighbor, or the focused item. | Selected edge runs stay put; a single item wraps. |
| **Go to first thought** | Moves Board focus to the first live thought. | Does not select it. |
| **Go to last thought** | Moves Board focus to the last live thought. | Does not select it. |
| **Expand or collapse thought** | Toggles durable compact presentation. | Canonical text does not change. |

## Editing and transformation

| Command | What it does | Important condition or result |
| --- | --- | --- |
| **Insert plain newline** | Inserts a newline without smart-list continuation. | Edit only. |
| **Delete logical line** | Deletes the current logical line. | One editor revision. |
| **Delete sentence** | Deletes Unicode sentences touched by cursor or selection. | Folded targets are revealed unchanged first; review and repeat to delete. |
| **Jump cursor up 5 visual rows** | Moves up through rendered rows. | Visual wrapping determines the destination. |
| **Jump cursor down 5 visual rows** | Moves down through rendered rows. | Stops at the thought's last visual row. |
| **Extend selection to visual row start** | Selects to the current wrapped row start. | Edit only. |
| **Extend selection to visual row end** | Selects to the current wrapped row end. | Edit only. |
| **Move cursor to thought beginning** | Moves to byte position zero of the thought. | Edit only. |
| **Move cursor to thought end** | Moves to the exact end of the thought. | Edit only. |
| **Indent line or selection** | Nests recognized list content or inserts spaces. | Width comes from configuration. |
| **Outdent line or selection** | Outdents recognized list content. | Ordinary unrelated text remains unchanged. |
| **Split thought at cursor** | Divides one thought at the exact cursor. | Requires Edit with no selection. |
| **Extract selection as new thought** | Moves selected text into a neighboring thought. | Requires a nonempty editor selection. |
| **Merge selected thoughts** | Joins a contiguous thought range. | Uses the exact configured merge separator. |

## Selection

| Command | What it does | Important condition or result |
| --- | --- | --- |
| **Select all items** | Selects every live Board item. | In Edit, the corresponding shortcut selects text instead. |
| **Toggle item selection** | Adds or removes the focused item from an arbitrary selection. | Focus and selection stay distinct. |
| **Start contiguous range selection** | Latches a stable range anchor. | Move or click to extend; Escape clears. |

## Delivery and transfer

| Command | What it does | Important condition or result |
| --- | --- | --- |
| **Submit** | Delivers ordered bodies and removes unchanged sources after acceptance. | Verified Herdr target required. |
| **Submit and keep** | Delivers the same payload and retains every source. | Verified Herdr target required. |
| **Submit to agent...** | Searches verified agents on the current Herdr server. | Does not address arbitrary panes or remote servers. |
| **Submit all** | Delivers every live thought and removes sources after acceptance. | Empty, failed, or ambiguous delivery preserves the Board. |
| **Submit all and keep** | Delivers every live thought and retains the Board. | Separators and names are omitted from payload. |
| **Send to another Proqi session** | Copies eligible selected thoughts in order into another durable Proqi session. | The selected destination cohort commits as one unit; separators stay in the source. |
| **Send to another Proqi session and remove thought** | Copies the complete selected cohort, then removes its sources after destination durability. | Source removal is one undoable unit; undo leaves destination copies intact. |
| **Export to file...** | Writes the focused thought or selected thoughts to one plain-text file, exactly as copying them would. | Next release. Opens a path field; the Board is unchanged. |
| **Export to file and remove...** | Writes the file, then removes the exported thoughts. | Next release. Removal happens only after the file is durable and is one undoable unit; undo keeps the file. |
| **Export to file and replace with reference...** | Writes the file, then replaces the exported thoughts with one thought holding the file path, shown as `[File N]`. | Next release. One undoable unit; undo restores the thoughts and keeps the file. |
| **Refresh adjacent agents** | Rediscovers eligible Herdr targets. | Does not send a prompt. |

## Sessions

| Command | What it does | Important condition or result |
| --- | --- | --- |
| **Rename session** | Sets or clears the current session's optional name. | Browser history owns the durable operation. |
| **Copy session ID** | Copies the complete canonical session identifier. | No durable mutation. |
| **Copy resume command** | Copies an exact command for resuming this session. | No durable mutation. |

## Attachments and capture

| Command | What it does | Important condition or result |
| --- | --- | --- |
| **Refresh attachments** | Rechecks current attachment accessibility. | Never requests an iCloud download. |
| **Enable Screenshot Inbox** | Starts the bounded macOS watcher. | Label changes to disable, resume, or unavailable as state changes. |
| **Retry Screenshot Capture** | Retries one retained failed capture. | Appears only while a retry is available. |

## Invocations, updates, and recovery

| Command | What it does | Important condition or result |
| --- | --- | --- |
| **Insert discovered invocation** | Opens discovery and inserts the selected invocation. | Requires an editable thought. |
| **Refresh invocations** | Rescans supported local invocation roots. | Does not change thought content. |
| **Check for updates** | Queries the verified stable installation channel. | Explicit checks work even when startup checks are disabled. |
| **What's new** | Opens installed release highlights. | Describes the installed product. |
| **Retry failed save** | Retries pending persistence after a storage failure. | Recovery-critical action. |
| **Export recovery file** | Writes optimistic in-memory state to a new private file. | Never overwrites an existing file. |
| **Undo** | Moves the active history owner backward. | Availability and label are contextual. |
| **Redo** | Moves the active history owner forward. | New divergent edits clear the relevant redo branch. |
| **Open contextual help** | Shows effective controls for the active owner. | Uses configured bindings. |
| **Quit Proqi** | Exits through durability handling. | Recovery-critical; does not discard failed work silently. |

## Actions that are not Commands rows

Navigation, text cursor movement, confirmation, close, query editing, Browser
trash, and direction choice are direct contextual controls. They are registered
semantic actions, but they are not additional searchable Commands entries.
Their behavior is covered by [Keyboard map](keyboard.md) and the relevant task
guides.
