# Get started

<span class="version-scope">Proqi 0.11.0</span>

## Install

On macOS, Homebrew is the recommended route:

```sh
brew install oborchers/tap/proqi
```

On macOS or Linux, the standalone installer selects the native archive,
verifies its checksum and version, and installs without `sudo`:

```sh
curl -LsSf https://github.com/oborchers/proqi/releases/latest/download/proqi-installer.sh | sh
```

It defaults to `$HOME/.local/bin` and does not edit `PATH`. Set
`PROQI_INSTALL_DIR` to another absolute directory below `$HOME` if needed.

Rust users can install with Rust 1.88 or newer:

```sh
cargo install proqi --locked
```

<span class="version-scope">Next release</span> In
[Herdr](https://github.com/herdrdev/herdr) 0.8.0 or newer, the Proqi plugin
installs Proqi when it is missing and adds a toggle that opens Proqi beside
the focused pane:

```sh
herdr plugin install oborchers/proqi
```

An existing Homebrew, standalone, Cargo, or Debian installation is used as is.
See [Use Proqi as a Herdr plugin](guides/herdr-plugin.md).

Proqi supports the macOS and Linux targets published on the
[latest release](https://github.com/oborchers/proqi/releases/latest). Only the
latest pre-1.0 minor release is supported.

## Open a board

```sh
proqi                         # create a new board
proqi -c                      # continue the latest inactive board here
proqi -r                      # open the searchable session browser
proqi -r <id-or-name>         # resume one exact session
```

The first eligible fresh interactive session in a pristine store contains a
small practice board. Those are ordinary persisted thoughts. Resume,
noninteractive commands, and later new sessions do not create another practice
board.

## Learn the two main contexts

**Board** is for moving among thoughts, selecting groups, reordering, copying,
and delivery. **Edit** is for changing one thought's exact text.

- Press `n` or activate **+ New thought** to create.
- Press `j` and `k`, or the arrow keys, to move Board focus.
- Press `Enter` or `e`, or click body text, to edit.
- Press `Escape` to leave Edit and return to Board.
- Press `:` for Commands and `?` for contextual Help.

On a genuinely empty board, typing or pasting creates the first thought. Press
`Escape` first when you want Board commands instead of immediate composition.
If the public JSON API adds the first item while that empty Compose prompt is
open, Proqi focuses the item in Board mode. Use `j`, `k`, or the arrows immediately;
press `Enter` or `e` when you want a text caret.

## Understand saving

There is no save command. Accepted changes are written automatically. The
footer distinguishes pending, durable, failed, and retry states based on actual
persistence acknowledgement.

Quit waits for durability. A storage failure keeps optimistic work visible and
offers retry or private recovery export rather than pretending that the data is
safe. See [Sessions and recovery](guides/organization-and-recovery.md).

## Understand Primary

Documentation uses **Primary** for the platform's main application modifier:

- macOS: logical Command, reported as Super or Meta
- Linux and other portable policies: logical Control

Raw Control remains a separate modifier on macOS. The operating system,
keyboard remapper, terminal, and multiplexer all get a chance to consume or
rewrite a key before Proqi receives it. See
[Configure and troubleshoot shortcuts](guides/shortcuts.md).

## Add the agent skills

The optional Proqi skills let a coding agent use the JSON CLI for a session you
name. Install them with the [Agent Skills CLI](https://github.com/vercel-labs/skills):

```sh
npx skills add oborchers/proqi --skill proqi -g --agent codex --agent claude-code
```

In Claude Code, you can use the plugin marketplace instead:

```text
/plugin marketplace add oborchers/proqi
/plugin install proqi@proqi
```

The plugin installs both `proqi` and `proqi-debug`. Neither method installs the
Proqi executable. See
[Install the shipped agent skills](reference/cli.md#install-the-shipped-agent-skills).

## Continue learning

- [Capture and edit](guides/capture-and-edit.md)
- [Select and act on thoughts](guides/selection.md)
- [Complete feature index](reference/features.md)
