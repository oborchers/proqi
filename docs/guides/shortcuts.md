# Configure and troubleshoot shortcuts

> Applies to Proqi 0.11.0.

Proqi receives logical key events only after macOS or Linux, a keyboard
remapper, the terminal host, and any terminal multiplexer have handled them.
The label printed on a key does not prove which event reached Proqi.

## Understand Primary and Control

`Primary` means logical `Cmd`, reported as Super or Meta, on macOS. It means
logical `Ctrl` on portable platforms. Raw Control is not a second spelling of
Primary on macOS.

The most important effective factory differences are:

| Action | macOS | Portable platforms |
| --- | --- | --- |
| Undo | `Ctrl+Z`; retained `Primary+Z`; Board `u` | `Primary+Z`; Board `u` |
| Redo | `Ctrl+Shift+Z` or `Ctrl+Y`; retained Primary aliases | `Primary+Shift+Z` or `Primary+Y` |
| Deliver and remove | `Ctrl+Enter`; retained `Primary+Enter`; Board `s` | `Primary+Enter`; Board `s` |
| Deliver and keep | `Ctrl+Shift+Enter`; retained `Primary+Shift+Enter`; Board `Shift+S` | `Primary+Shift+Enter`; Board `Shift+S` |
| Reorder | `Option+Shift` plus vertical direction; retained Primary forms | `Primary+Shift` plus vertical direction |
| Paste and clean up | `Primary+Shift+V`; Board `Shift+P` | `Primary+Shift+V`; Board `Shift+P` |

Help and footer labels prefer terminal-safe macOS Control aliases where
appropriate. The complete factory map remains in the
[README controls](../../README.md#board-controls).

## Remap one action

Use keymap schema 1 in the platform-native `config.toml`:

```toml
[keymap]
schema_version = 1

[keymap.bindings.board]
"submission.submit_remove" = [
  { key = "Enter", modifiers = ["Primary"] },
  { key = "s" },
  { key = "F5" },
]
"submission.submit_keep" = []
"thought.delete" = [{ key = "Delete" }, { key = "d" }]

[keymap.macos.edit]
"submission.submit_remove" = [{ key = "Enter", modifiers = ["Super", "Alt"] }]

[keymap.portable.edit]
"submission.submit_remove" = [{ key = "F5" }]
```

A supplied context and action list replaces every factory alias for that pair.
It does not append. An empty list disables keyboard access where doing so is
safe. Omitted pairs keep their defaults. Platform-specific lists replace the
common list for the same pair.

Proqi validates both platform graphs before terminal setup. Unknown actions,
collisions, printable bindings that would steal text, loss of invariant
`Escape`, and removal of required recovery routes are errors. Proqi never
rewrites the configuration automatically.

Use the [versioned shortcut contract](../../context/SHORTCUTS.md) for the full
schema and the [keymap action inventory](../../context/KEYMAP_ACTIONS.md) for
stable action and context identifiers.

## See what Proqi actually receives

Run the bounded keypress diagnostic in the same terminal path and context where
the shortcut should work:

```sh
proqi diagnostics keypress --context board,edit --timeout-ms 5000
proqi --json diagnostics keypress --context board --defaults
```

The diagnostic reports one logical key, its exact modifiers, the selected
context stack, and the resolved action. `Escape` cancels. `--defaults` bypasses
user configuration, which is useful when normal startup rejects an invalid
file.

A timeout means no key event reached the diagnostic. Proqi cannot determine
whether the operating system, a remapper, the terminal, Herdr, or another layer
consumed it. The diagnostic records no paste payload, thought content, session
content, or raw terminal response, and it restores terminal state on every exit
path.

## Ghostty on macOS

Inspect the defaults of the installed Ghostty version:

```sh
ghostty +list-keybinds --default
```

Ghostty can consume `Cmd+Enter` for fullscreen and `Cmd+Shift+Enter` for split
zoom before Proqi sees them. It can also rewrite Command plus horizontal arrows
to raw Control events. Add only the overrides you want Ghostty to forward:

```ini
keybind = super+enter=unbind
keybind = super+shift+enter=unbind
keybind = super+arrow_left=unbind
keybind = super+arrow_right=unbind
keybind = super+shift+v=csi:118;10u
```

The final line emits logical Super plus Shift plus `v` for **Paste and clean
up**. It is a verified example for the tested Ghostty and Crossterm path, not a
promise for every layout, remapper, or terminal version. Proqi never edits
Ghostty, keyboard, shell, or operating-system configuration.

Bracketed paste performed by the host always uses exact paste. Spacing cleanup
requires the distinct cleanup action after a suitable logical key reaches
Proqi.
