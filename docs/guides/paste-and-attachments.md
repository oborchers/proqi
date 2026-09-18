# Paste, clean up, and attach files

> Applies to Proqi 0.11.0.

Exact paste is always the default. Cleanup is a separate deliberate action, so
Proqi never guesses that an unmarked newline came from visual wrapping.

## Paste exact content

Use `Primary+V`, Board `p`, native bracketed paste, or **Paste exactly** in
Commands. In Board mode with no selection, the result becomes one new thought
and opens in Edit. In Edit, it replaces the current text selection or inserts
at the cursor.

Exact paste preserves whitespace, blank lines, Unicode, and LF or CRLF line
endings. One paste is one persistent undo unit.

## Paste and clean up spacing

Use `Primary+Shift+V`, Board `Shift+P`, or **Paste and clean up** in Commands.
For ordinary prose, cleanup:

- converts runs of horizontal whitespace to one ASCII space;
- trims ordinary prose line edges;
- keeps a single authored line break;
- keeps one blank paragraph line and collapses longer blank runs.

Recognized Markdown list markers, nesting, and line boundaries stay intact.
Fenced and indented code, tables, block quotes, paths, URLs, unsupported
controls, and blocks with protected semantic annotations stay exact.

Whitespace-only cleanup does not create a thought and does not replace a text
selection. If transformation fails, Proqi pastes the original payload exactly
and reports the fallback. The complete cleanup is one undo and redo step.

To clean an existing thought, focus it and press Board `f`, or choose **Clean
up spacing** in Commands. In Edit, `Ctrl+Shift+F` cleans the complete active
thought, not only the text selection. An unchanged result creates no history
entry.

## Drop files and paste images

Dragging local files into Proqi inserts their absolute paths. Board mode creates
one thought, while Edit inserts into the active thought. Proqi accepts the
conversion only when every referenced item resolves to an existing local file.
Otherwise it preserves the original text exactly.

When the native clipboard contains raw image pixels, exact paste writes a
private PNG inside the current Proqi session and inserts its absolute path.
Ordinary dropped files remain in their original locations. Proqi does not copy,
upload, or inspect their contents automatically.

Images and files render as compact `[Image N]` and `[File N]` placeholders, but
their exact absolute paths remain the canonical prompt content used for copy,
search, recovery, and agent delivery. Large pasted context folds similarly once
it reaches 12 logical lines or 1,200 perceived Unicode characters.

On macOS, annotated copy and cut preserve both plain text and annotation
metadata through a generation-bound typed clipboard item. Other supported
platforms reject annotated copy and cut rather than silently dropping metadata.
Unannotated text still has native clipboard and bounded OSC 52 paths.

An attachment marked inaccessible, in iCloud, or downloading is not ready for
direct delivery. Proqi can refresh the status, but it never requests an iCloud
download. The path still refers to an external file that can disappear after a
successful check.

## Screenshot Inbox on macOS

Screenshot Inbox watches for new screenshots. It does not take screenshots.
On an empty board, press `Escape` before the factory Board key `i`. On an
ordinary Board, press `i` directly. You can also choose **Enable Screenshot
Inbox** in Commands.

At activation, Proqi snapshots the watched directory and ignores existing
files. Each later completed accepted PNG, JPEG, or TIFF becomes one durable
thought containing the exact absolute source path followed by one ordinary
space. The image path folds visually and the editor is ready for a caption
after that space.

The default directory is the current user's Desktop. Configure an isolated
directory when desired:

```toml
[screenshot_inbox]
# directory = "/absolute/path/to/an/isolated/inbox"
# Example filename fallbacks, which match the complete filename:
# filename_patterns = ["Screenshot *.png", "Screen Shot *.png"]
capture_all_new_images = false
# inactivity_timeout_minutes = 20
# max_unattended_captures = 10
notify_terminal_on_auto_pause = false
```

The default classifier uses macOS screenshot metadata. Filename patterns are
user-configurable fallbacks. `capture_all_new_images = true` deliberately
accepts every otherwise valid new image in the watched directory.

Only one Proqi process can listen at a time. A compatible contender offers
**Cancel** or **Take over**. Takeover asks the verified owner to reconcile,
finish admitted work, stop watching, and release authority. It never force
unlocks a live or incompatible owner.

By default, every listening period pauses after 20 minutes without deliberate
Proqi input or after 10 unattended admitted captures. Keyboard input, paste,
clicks, drag, and scrolling renew the bounds. Resize, host focus, pointer
motion, and watcher activity do not. Resume starts a fresh baseline, so files
accumulated during the pause are not imported later. Restart begins with
Screenshot Inbox off.

Use Commands to disable or resume the inbox. A failed capture remains available
through **Retry Screenshot Capture**. Disabling, pausing, taking over, or
quitting does not silently retry it. Proqi asks before quitting when doing so
would abandon a retained failed capture.

macOS may request Files & Folders access for the terminal host that runs Proqi.
It does not require Screen Recording or Accessibility permission. Linux starts
no watcher and reports that Screenshot Inbox is available on macOS only.

## Deliver attachments safely

Before direct agent delivery, Proqi freshly verifies every attachment in the
captured source thoughts. If any file is unavailable, downloading, in iCloud,
timed out, or otherwise unreadable, Proqi sends nothing and removes nothing.
There is no bypass for an annotated unavailable attachment.

See [Deliver prompts to agents](agent-delivery.md) for receipt and removal
behavior.
