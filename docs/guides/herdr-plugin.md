# Use Proqi as a Herdr plugin

<span class="version-scope">Proqi 0.14.0</span>

The plugin needs Proqi 0.14.0 or newer, Herdr 0.8.0 or newer, and macOS or
Linux. If an older Proqi is already installed, upgrade it through its existing
installation channel before using the toggle.

Proqi is an agent-optimized terminal scratchpad for follow-up prompts next to
coding-agent sessions. The Herdr plugin adds one action that opens Proqi to the
right of the focused pane, focuses it when it is already open, and closes it
when it is focused. In the next release, a second action captures the terminal
selection or the clipboard text into the tab's Proqi session without switching
to it; see [Capture the selection or clipboard](#capture-the-selection-or-clipboard).

## Install

```sh
herdr plugin install oborchers/proqi
```

Herdr shows a preview of the plugin and the command it will run before
anything happens. After you confirm, the plugin's build step checks for Proqi:

- When `proqi` is on the `PATH` of the shell running the install, or in the
  standalone installer's directory (`$PROQI_INSTALL_DIR`, default
  `$HOME/.local/bin`), nothing is installed. An existing Homebrew, Cargo,
  Debian, or standalone installation is never replaced or shadowed, and it
  keeps its own update channel.
- Otherwise it downloads `proqi-installer.sh` and `proqi-installer.sh.sha256`
  from the latest GitHub Release with `curl`, checks the installer against that
  SHA-256 record, and runs it. The installer is the same one the
  [standalone instructions](../getting-started.md#install) use. It verifies the
  archive checksum and version and installs `proqi` below `$HOME` without
  `sudo`. Proqi's own updater then manages that installation.

The installer and its checksum come from the same GitHub Release. The check
therefore proves that the installer arrived intact, not who published it. The
trust anchor is GitHub over HTTPS, exactly as for
`curl -LsSf https://github.com/oborchers/proqi/releases/latest/download/proqi-installer.sh | sh`.

The build step requires `curl` and `sha256sum` or `shasum`. Without network
access it fails, Herdr aborts the installation, and nothing is registered. Run
the same command again once you are online.

Herdr shows build output only when the build fails. After a fresh install,
`proqi` lives in `$HOME/.local/bin`. The plugin finds it there even when that
directory is not on your `PATH`; add it to your shell's `PATH` to run `proqi`
yourself.

### When Herdr cannot find Proqi

The toggle runs with the environment of the Herdr server, not of your current
shell. If you installed Proqi with Homebrew or Cargo and the Herdr server was
started without that directory on its `PATH`, the toggle reports that Proqi is
not on the Herdr server's `PATH`. Restart the Herdr server from a shell where
`command -v proqi` prints a path.

If an older Proqi is installed, update it through the channel you installed it
with. The action reports a Proqi that does not provide `proqi herdr toggle`
instead of running it.

## Bind a key

Herdr does not bind plugin actions automatically. Add a binding to Herdr's
`config.toml`, then reload the configuration:

```toml
[[keys.command]]
key = "prefix+i"
type = "plugin_action"
command = "proqi.toggle"
description = "toggle Proqi"
```

`prefix+i` is unbound in Herdr's default keymap; choose any free key. You can
also run the action from a shell with `herdr plugin action invoke proqi.toggle`.

## What the toggle does

The toggle acts only on the current tab, and each tab has at most one Proqi
pane.

| Situation | Result |
| --- | --- |
| No Proqi pane in the tab | Opens Proqi to the right of the focused pane |
| A Proqi pane exists but is not focused | Focuses it |
| The Proqi pane the plugin opened is focused | Saves pending edits, then closes the pane once Proqi confirms |
| A Proqi you started yourself is focused | Returns focus to the tab's agent; it is never closed |
| The Proqi pane is left over from a Herdr restart | Opens the same session in a new pane and closes the leftover shell if it is still idle |

### Which session a tab uses

Each tab keeps one Proqi session. The plugin remembers it, so every later
toggle in the tab reopens the same session, whichever pane or subdirectory is
focused. Closing the pane never deletes the session.

For a tab's first companion, the plugin finds or creates a session with the
same get-or-create rule as `proqi sessions ensure`:

- The name is the agent's name when exactly one agent in the tab has a Herdr
  name, for example `api-claude` for an agent started with
  `herdr agent start api-claude` or renamed with `herdr agent rename`. Unnamed
  agents do not count.
- Otherwise the name is the tab label. Herdr labels unnamed tabs with their
  position, which changes when tabs close or move, so a numeric label is
  replaced by the tab's stable identity, for example `api-w2-t4` for tab
  `w2:t4` in workspace `api`.
- If Herdr cannot list the tab's agents, the toggle opens nothing and shows a
  notification; try again. It does not fall back to the tab label, because the
  plugin remembers the session it picks. The same rule picks a new session
  when the remembered one was trashed or deleted.
- The origin is the Herdr worktree checkout for a worktree workspace, else the
  Git repository root containing the focused pane's directory, else that
  directory itself. Splits in subdirectories of one repository therefore share
  one origin. Herdr's own workspace directory follows the focused pane, so it
  is not used.

The plugin reuses an existing session only when this rule produces exactly its
name and origin. It does not adopt sessions named by other tools. A Proqi that
another tool keeps open in the tab is recognized and focused instead.

If the session is already open in another pane, for example because two
workspaces have tabs with the same label or agents with the same name in the
same repository, the toggle shows a Herdr notification and does not start a
second Proqi. Rename the tab or its agent, whichever named the session, or
close the other pane first.

If the tab's name already belongs to a Proqi session from a different
directory, for example two repositories that both have a tab labeled `main` or
an agent named `main`, the toggle reports a name conflict instead of guessing.
Rename the tab, or the agent with `herdr agent rename`, whichever named the
session, or rename the other session with `proqi sessions rename`.

### Closing and recovery

The toggle closes Proqi only after Proqi confirms that your edits are saved.
Without that confirmation, for example while Proqi is still starting, it keeps
the pane open; toggle again or quit Proqi with its own Quit action.

If the toggle is interrupted after opening Proqi but before recording that
pane, for example because the plugin cannot write its private state or its
process is killed, the new Proqi is treated like one you started yourself. It
stays open and is focused, never closed. Quit it with Proqi's Quit action or
close the pane with Herdr. An empty shell left over from a Herdr restart also
stays open in that case until a later toggle replaces it or you close it.

## Capture the selection or clipboard

<span class="version-scope">Next release</span>

A second action, **Capture to Proqi** (`proqi.capture`), stores text as one new
thought at the end of the tab's Proqi session. It never opens, focuses, or
switches to Proqi, and the tab does not need a Proqi pane. Herdr does not bind
it automatically; add a binding next to the toggle's:

```toml
[[keys.command]]
key = "f6"
type = "plugin_action"
command = "proqi.capture"
description = "capture to Proqi"
```

`f6` is only a suggestion; choose any free key. The action needs the Proqi
release that provides `proqi herdr capture`. An older Proqi reports that it is
too old and captures nothing.

### What it captures

1. The terminal selection Herdr passes with the key-bound invocation, when it
   is not empty.
2. Otherwise the clipboard's text.

It never reads a pane's scrollback instead. The thought holds exactly the
captured text: nothing is trimmed, quoted, wrapped, or added, and line endings,
indentation, and Unicode stay as they were. Capturing the same text twice
stores it twice.

In practice the clipboard is the usual source. Herdr's default
`copy_on_select = true` copies a mouse selection to the clipboard and clears it
when you release the button, so select the text, release, then press the key.
Herdr passes a selection to a plugin action only for a key binding pressed in
a Herdr client, and today its prefix key and copy mode clear the selection
before the action starts, which Herdr tracks as
[herdrdev/herdr#3380](https://github.com/herdrdev/herdr/issues/3380). Running
the action with `herdr plugin action invoke proqi.capture` never passes a
selection and always uses the clipboard.

### Which session receives it

The same rule as the toggle picks the session: the tab's recorded session,
unless it was trashed or deleted, otherwise the tab's named session found or
created as described in [Which session a tab uses](#which-session-a-tab-uses).
When the tab had no recorded session yet, it now records the one it captured
into, so the next toggle opens it.

When that session is open in Proqi, the capture reaches the running Proqi,
which appends it quietly: your editor, caret, selection, and any open overlay
stay where they are, and the footer counts `1 new capture`, `2 new captures`,
and so on, as for the Screenshot Inbox. When no Proqi has the session open, the
thought is saved directly and appears when you open it.

### Feedback

A Herdr notification confirms each capture with a single-line preview of at
most 40 characters and the character count, for example:

```text
Captured to Proqi: "Review the retry path in the export work…" (184 characters)
```

The preview drops control characters and joins lines with spaces. When nothing
can be captured, nothing is stored or created and the notification says why:

| Notification | Cause |
| --- | --- |
| `Nothing captured: the selection is empty` | The selection held only spaces or line breaks |
| `Nothing captured: the clipboard is empty` | The clipboard held no text, or only spaces or line breaks |
| `Nothing captured: the clipboard holds no text (for example an image)` | The clipboard held an image or other non-text content |
| `Nothing captured: the clipboard has N bytes, more than the 131072-byte thought limit` | The text exceeds the 128 KiB thought limit; nothing is truncated |
| `Nothing captured: the clipboard could not be read (...)` | The system clipboard was unavailable |

Session and Herdr failures, such as a name conflict or an unreachable Herdr,
use the same `Nothing captured:` prefix with the toggle's explanation.

### Remote Herdr servers

Plugin actions run on the Herdr server. When your Herdr client is attached to a
server on another machine, the clipboard fallback reads that server's
clipboard, not the one on the machine in front of you. Only a selection that
Herdr passes with the invocation reaches Proqi from your screen. Proqi does not
request the local clipboard through terminal escape sequences.

## Limitations

- Herdr does not restore plugin panes after a cold restart of its server. The
  pane comes back as an empty shell. The next toggle recognizes it, reopens the
  same Proqi session beside the agent, and closes the shell only if it is still
  idle at that moment. It never closes a pane that runs anything else. Text
  typed at that shell's prompt but never submitted is not visible to Herdr and
  is discarded with the shell.
- The plugin opens Proqi to the right of the focused pane. Move or resize the
  pane with Herdr's normal pane commands.
- Herdr can focus an arbitrary pane only by zooming it. The plugin reads the
  tab's zoom state first: an unzoomed tab ends unzoomed, and a zoomed tab stays
  zoomed on the newly focused pane.
- If Herdr cannot report a pane's process in time, that pane might hide a
  Proqi, so the toggle does not open another one. It names the pane in its
  notification; try again, or close that pane if it stays unresponsive.
  Focusing and closing an existing Proqi still work. In a tab with many panes
  on a Herdr server that answers slowly, the toggle keeps refusing until Herdr
  reports every pane within its twelve-second window.
- When the focused pane runs a Proqi you started yourself, the toggle returns
  focus to the tab's only agent. With several agents in the tab it reports that
  instead of guessing.

## Update and uninstall

Herdr has no plugin update command. Run `herdr plugin install oborchers/proqi`
again to refresh the plugin. Proqi itself updates through its own channel, and
reinstalling the plugin never replaces an existing Proqi.

```sh
herdr plugin uninstall proqi
```

Uninstalling removes the plugin and its checkout. It keeps Proqi, your
sessions, and any Proqi the build step installed.

## See also

- [Deliver prompts to agents](agent-delivery.md)
- [`proqi herdr toggle`](../reference/cli.md#toggle-proqi-beside-a-herdr-agent)
- [`proqi herdr capture`](../reference/cli.md#capture-into-a-herdr-tabs-session)
