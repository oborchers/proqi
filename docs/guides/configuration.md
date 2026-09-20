# Configure appearance and behavior

<span class="version-scope">Proqi 0.11.0</span>

Proqi works without configuration. Optional settings live in the
platform-native Proqi configuration directory as `config.toml`. Invalid
configuration fails before terminal setup rather than partially applying.

The default file is `$HOME/Library/Application Support/proqi/config.toml` on
macOS. On Linux it is `$XDG_CONFIG_HOME/proqi/config.toml`, or
`$HOME/.config/proqi/config.toml` when `XDG_CONFIG_HOME` is unset.

## Core settings

```toml
check_for_updates = true
show_session_id = false
smart_lists = true
list_indent_width = 2
merge_separator = "\n\n"
keyboard_enhancement = "auto"
mouse_capture = true
density = "comfortable"
theme = "auto"
```

| Setting | Shipped values and effect |
| --- | --- |
| `check_for_updates` | Enables the content-free automatic stable-release check. Explicit checks remain available when false. |
| `show_session_id` | Shows the complete canonical session ID beside the footer name only when it fits. |
| `smart_lists` | Continues recognized Markdown list structure on Enter. |
| `list_indent_width` | Sets spaces per list indentation level. |
| `merge_separator` | Exact text inserted between bodies during Merge. |
| `keyboard_enhancement` | `auto` negotiates only compatible terminal flags; `disabled` uses portable events. |
| `mouse_capture` | Requests xterm SGR mouse reporting. Disable if the terminal or multiplexer mishandles it. |
| `density` | `comfortable` or `compact`; shallow boards automatically use compact spacing. |
| `theme` | `auto`, `light`, `dark`, `limited`, or a bounded local theme file. |

`list_indent_width` accepts 1 through 8 spaces. `merge_separator` must contain
1 through 1,024 UTF-8 bytes and is inserted without normalization.

## Theme safely

Automatic mode inherits terminal foreground and background while resolving
semantic roles for focus, dividers, links, annotations, success, warning, and
error. Light and dark select explicit built-ins. Limited mode uses a
terminal-native reduced palette.

A custom theme file can override semantic `#RRGGBB` roles. Unsafe contrast,
unknown fields, malformed colors, oversized files, and unsafe paths are
rejected. Focus never relies on color alone.

Inline `[theme_overrides]` and a schema 1 theme file support these roles:
`foreground`, `background`, `accent`, `accent_surface`, `on_accent`, `muted`,
`divider`, `focused_surface`, `link`, `annotation`, `success`, `warning`, and
`error`. Set `focused_surface = "none"` when only the non-color focus cue is
wanted. Inline roles override the selected custom file. The limited theme
cannot be combined with overrides.

Use the checked-in [dark theme example](../themes/proqi-dark.toml) as a starting
point.

## Configure Screenshot Inbox

```toml
[screenshot_inbox]
# directory = "/absolute/path/to/an/isolated/inbox"
# filename_patterns = ["Screenshot *.png", "Screen Shot *.png"]
capture_all_new_images = false
supported_types = ["png", "jpeg", "tiff"]
min_file_bytes = 64
max_file_bytes = 67108864
max_dimension = 16384
max_pixels = 100000000
debounce_ms = 350
inactivity_timeout_minutes = 20
max_unattended_captures = 10
notify_terminal_on_auto_pause = false
```

The directory must be absolute. Filename patterns match complete filenames and
act as fallbacks to the macOS metadata classifier. `capture_all_new_images`
deliberately broadens capture to every otherwise valid new image.

The configurable safety limits are bounded: at most 32 filename patterns of
160 Unicode scalars each; file size no larger than 512 MiB; dimension no larger
than 65,535; at most one billion pixels; debounce from 100 to 1,000 ms;
inactivity from 1 to 1,440 minutes; and unattended captures from 1 to 100.
Image types are validated by content, not only filename.

See [Paste and attachments](paste-and-attachments.md#screenshot-inbox-on-macos)
for lifecycle and privacy behavior.

## Configure shortcuts

Keymap schema 1 supports common bindings plus macOS and portable overrides.
Each supplied action/context list replaces its defaults. See
[Configure and troubleshoot shortcuts](shortcuts.md) before changing it.

The legacy `[keybindings]` table remains accepted through explicit translation,
but it cannot be mixed with `[keymap]`.

## Add invocation roots

Built-in discovery already covers the documented roots for Agent Skills,
Codex, Claude Code, OpenCode, and Pi. Additional local roots use entries like:

```toml
[[invocation_roots]]
path = "/absolute/path/to/definitions"
kind = "skill"       # skill, command, or agent
harness = "configured"
scope = "global"     # project or global
```

Supported harness values are `agent_skills`, `codex`, `claude_code`,
`open_code`, `pi`, and `configured`. A project path may be relative to the
current directory; a global path must be absolute. URLs, control characters,
plugin scope, paths longer than 1,024 characters, and more than 32 additional
roots are rejected.

Discovery reads bounded definition metadata and does not execute definitions.
See [Discover commands, skills, and collaborators](discovery-and-invocations.md).

## Configuration safety

`config.toml` must be a regular local file no larger than 64 KiB. Startup makes
its permissions private before loading it. Unknown fields and malformed TOML
are errors. Theme files are local regular files with the same size bound; remote
theme URLs are unsupported.
