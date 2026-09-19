# Keyboard and pointer map

<span class="version-scope">Factory behavior in Proqi 0.11.0</span>

The map below is a discovery summary. Contextual Help is the authoritative view
of the bindings effective in the running application after configuration.

`Primary` means Command on macOS and Control on portable platforms. Raw Control
remains distinct on macOS.

## Board essentials

| Intention | Keyboard | Pointer equivalent |
| --- | --- | --- |
| Create | `n`, insertion-row `Enter`, or paste | Click the insertion row |
| Focus | `j`, `k`, or vertical arrows | Click an item |
| Edit | `Enter` or `e` | Click body text |
| Select one | `Space` | Click the selection control |
| Extend range | Shift plus vertical movement, `Shift+J`, `Shift+K`, or latch with `v` | Shift-click an endpoint |
| Select all | `a` or `Primary+A` | Commands |
| Reorder one | Primary+Shift plus vertical direction; macOS also Option+Shift | Drag the focused item |
| Copy or safe cut | `Primary+C` / `y`; `Primary+X` / `x` | Visible controls when available |
| Delete | `d` or unmodified Delete | Visible delete control |
| Duplicate | `Primary+D` or `Shift+D` | Commands |
| Collapse | `c` | Click the collapse control |
| Merge contiguous thoughts | `t` | Commands |
| Clean up focused thought | `f` | Commands |
| Search / Commands / Help | `/` / `:` / `?` | Footer controls |
| Quit | `Primary+Q` or `q` | Quit control |

Focus is not selection. Keyboard and pointer group actions operate on the
selection when it exists and otherwise use focus. Reordering always moves only
the focused item.

## Editor essentials

| Intention | Factory input |
| --- | --- |
| Return to Board | `Escape` |
| Select all text | `Primary+A` |
| Copy, cut, exact paste | `Primary+C`, `Primary+X`, `Primary+V` |
| Paste and clean up | `Primary+Shift+V` |
| Delete logical line / sentence | `Primary+U` / `Primary+Shift+U` |
| Undo / redo | macOS Control history aliases plus retained Primary forms; portable Primary forms |
| Move by word | macOS Option plus horizontal arrow; portable Control plus horizontal arrow |
| Move to logical line edge | macOS Control plus horizontal arrow; portable Alt plus horizontal arrow; Home / End |
| Move to wrapped visual-row edge | macOS Command plus horizontal arrow |
| Extend any movement | Add Shift |
| Jump five rendered rows | Alt plus vertical arrow or Page Up / Page Down |
| Move to thought boundary | Control plus vertical arrow |
| Smart list / indent / outdent | Enter / Tab / Shift+Tab |
| Split or extract | `Primary+T`, or `Escape` then `t` using the captured cursor or selection |
| Submit remove / keep | Primary+Enter / Primary+Shift+Enter, with terminal-safe macOS Control aliases |

Mouse editing supports single-click cursor placement, character drag,
double-click word selection, triple-click logical-line selection, granular drag
extension, and Shift-click extension.

## Lists, overlays, and Browser

Arrow keys and `j` / `k` share navigation in non-text lists. Text queries keep
printable letters as content. Page keys or Alt plus vertical arrows jump through
long lists.

The Session Browser uses F2 to rename and F8 to trash while its query is empty.
Home and End jump to Browser boundaries. Direction choice accepts arrows or
`h`, `j`, `k`, and `l`.

Escape is the unconditional close route. A modal owner resolves its own
navigation before a colliding Board shortcut.

## Exact factory and configuration contracts

The repository keeps the detailed factory tables and stable versioned schema as
canonical sources:

- [README control tables](https://github.com/oborchers/proqi/blob/0b016f767af747f013301aaf41bcd0bf1cf827ef/README.md#board-controls)
- [Shortcut configuration and migration contract](https://github.com/oborchers/proqi/blob/0b016f767af747f013301aaf41bcd0bf1cf827ef/context/SHORTCUTS.md)
- [Complete action and context inventory](https://github.com/oborchers/proqi/blob/0b016f767af747f013301aaf41bcd0bf1cf827ef/context/KEYMAP_ACTIONS.md)

See [Shortcut troubleshooting](../guides/shortcuts.md) for remapping and host
delivery.
