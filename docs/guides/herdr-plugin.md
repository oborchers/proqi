# Use Proqi as a Herdr plugin

> Applies to the next Proqi release, which adds `proqi herdr toggle`. The plugin
> needs Proqi 0.13.0 or newer, Herdr 0.8.0 or newer, and macOS or Linux.

The Proqi Herdr plugin keeps a scratchpad for your next prompts beside the
agent you are already running, with verified delivery. One action opens Proqi
next to the focused agent pane, focuses it when it is already open, and closes
it when you are in it.

## Install

```sh
herdr plugin install oborchers/proqi
```

Herdr shows a preview of the plugin and the command it will run before
anything happens. After you confirm, the plugin's build step checks for Proqi:

- When `proqi` is on `PATH`, or in the standalone installer's directory
  (`$PROQI_INSTALL_DIR`, default `$HOME/.local/bin`), nothing is installed. An
  existing Homebrew, Cargo, Debian, or standalone installation is never
  replaced or shadowed, and it keeps its own update channel.
- Otherwise it downloads `proqi-installer.sh` and `proqi-installer.sh.sha256`
  from the latest GitHub Release, checks the installer against that SHA-256
  record, and runs it. The installer is the same one the
  [standalone instructions](../getting-started.md#install) use. It verifies the
  archive checksum and version and installs `proqi` below `$HOME` without
  `sudo`. Proqi's own updater then manages that installation.

The installer and its checksum come from the same GitHub Release. The check
therefore proves that the installer arrived intact, not who published it. The
trust anchor is GitHub over HTTPS, exactly as for
`curl -LsSf https://github.com/oborchers/proqi/releases/latest/download/proqi-installer.sh | sh`.

Without network access the build step fails, Herdr aborts the installation, and
nothing is registered. Run the same command again once you are online.

If an older Proqi is already installed, update it through the channel you
installed it with. The action reports a Proqi that does not yet provide
`proqi herdr toggle` instead of running it.

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
| The Proqi pane is left over from a Herdr restart | Opens the same session in a new pane and closes the leftover idle shell |

Each tab uses one named Proqi session, created with the same get-or-create
rule as `proqi sessions ensure`. The name is the tab label and the origin is
the focused pane's directory. Herdr's numeric default labels are qualified by
the workspace, so tab `2` in workspace `api` uses `api-2`. When you already
keep a Proqi session named after the tab for the same directory, the toggle
reuses it. Closing the pane never deletes the session; the next toggle reopens
it.

If the same session is already open in another pane, for example because two
workspaces have tabs with the same label and directory, the toggle shows a
Herdr notification and does not start a second Proqi. Rename one of the tabs,
or close the other pane first.

If the tab's name already belongs to a Proqi session from a different
directory, for example two repositories that both have a tab labeled `main`,
the toggle reports a name conflict instead of guessing. Rename one of the tabs,
or rename the other session with `proqi sessions rename`.

The toggle closes Proqi only after Proqi confirms that your edits are saved.
Without that confirmation, for example while Proqi is still starting, it keeps
the pane open; toggle again or quit Proqi with its own Quit action.

If the plugin cannot write its private state after opening Proqi, it reports
the error and leaves that Proqi open rather than risk your edits. Later toggles
focus it but never close it; quit it with Proqi's own Quit action. An empty
shell left over from a Herdr restart also stays open in that case until a later
toggle replaces it or you close it.

## Limitations

- Herdr does not restore plugin panes after a cold restart of its server. The
  pane comes back as an empty shell. The next toggle recognizes it, reopens the
  same Proqi session beside the agent, and closes the empty shell. It never
  closes a pane that runs anything else. Text typed at that shell's prompt but
  never submitted is not visible to Herdr and is discarded with the shell.
- The plugin opens Proqi to the right of the focused pane. Move or resize the
  pane with Herdr's normal pane commands.
- To focus a Proqi pane the plugin did not open, or to return to the agent,
  the plugin briefly zooms that pane, because Herdr has no command that focuses
  an arbitrary pane. A pane you had zoomed is left unzoomed.
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
