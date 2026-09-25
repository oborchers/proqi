# Use Proqi as a Herdr plugin

<span class="version-scope">Next release</span>

The plugin needs the first Proqi release that includes `proqi herdr toggle`,
Herdr 0.8.0 or newer, and macOS or Linux. Until that release is published, a
fresh install receives the latest published Proqi, and the toggle reports that
Proqi as too old instead of running.

Proqi is an agent-optimized terminal scratchpad for follow-up prompts next to
coding-agent sessions. The Herdr plugin adds one action that opens Proqi to the
right of the focused pane, focuses it when it is already open, and closes it
when it is focused.

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
  plugin remembers the first session it picks.
- The origin is the Herdr worktree checkout for a worktree workspace, else the
  Git repository root containing the focused pane's directory, else that
  directory itself. Splits in subdirectories of one repository therefore share
  one origin. Herdr's own workspace directory follows the focused pane, so it
  is not used.

The plugin reuses an existing session only when this rule produces exactly its
name and origin. It does not adopt sessions named by other tools. A Proqi that
another tool keeps open in the tab is recognized and focused instead.

If the session is already open in another pane, for example because two
workspaces have tabs with the same label in the same repository, the toggle shows a
Herdr notification and does not start a second Proqi. Rename one of the tabs,
or close the other pane first.

If the tab's name already belongs to a Proqi session from a different
directory, for example two repositories that both have a tab labeled `main`,
the toggle reports a name conflict instead of guessing. Rename one of the tabs,
or rename the other session with `proqi sessions rename`.

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
