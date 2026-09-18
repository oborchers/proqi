# Command-line reference

<span class="version-scope">Proqi 0.11.0</span>

Run `proqi --help` or append `--help` to any command for the installed contract.
Human output is for people. Add global `--json` when a script or coding agent
needs the versioned machine envelope.

CLI and JSON compatibility can change between minor releases before 1.0. Read
capabilities first and use standard input for thought bodies rather than shell
arguments.

## Common options

```text
proqi --help
proqi --version
proqi --json <command>
```

`--help` is also available on every command and subcommand. `--version` prints
the installed release. `--json` is global, so it can precede or follow a
command; examples keep it immediately after `proqi` for consistency.

## Interactive startup

```text
proqi
proqi --continue
proqi --resume [ID_OR_NAME]
```

`--continue` opens the latest inactive session ranked for the current working
directory. `--resume` without an argument opens the Session Browser. An explicit
reference accepts one canonical ID or unique session name.

## Discover capabilities

```sh
proqi --json capabilities
```

The response reports the current schema, identifier encoding, input bounds,
control protocol, transfer support, update support, and optional Herdr
capabilities. It is the starting point for automation.

## Generate shell completions

```sh
proqi completions bash
proqi completions fish
proqi completions zsh
```

Completion syntax is written to standard output. Release archives already
contain generated completions.

## Manage sessions

```text
proqi sessions [COMMAND]
proqi sessions list [--query TEXT] [--all]
proqi sessions rename <session> [NAME | --clear]
proqi sessions trash <session>
proqi sessions restore <session>
proqi sessions undo
proqi sessions redo
proqi sessions prune <session> --yes
```

With no subcommand, `sessions` lists resumable sessions. Ranking prefers the
current directory. Search covers optional names, launch paths, and thought
content. `--all` includes recoverably trashed sessions.

Rename, trash, and restore enter persistent Browser history. `sessions undo`
and `sessions redo` move that history. Prune is different: it permanently
deletes an already trashed session and requires `--yes`. It is not undoable.

## Inspect and change thoughts

```text
proqi thoughts list <session>
proqi thoughts inspect <session> <thought>
proqi thoughts add <session> [--position N] [--operation-id OP_ID]
proqi thoughts delete <session> <thought> [--operation-id OP_ID]
proqi thoughts rename <session> <thought> [NAME | --clear] [--operation-id OP_ID]
proqi thoughts replace <session> <thought> (--expected-sha256 HEX | --force) [--revision-id REV_ID]
proqi thoughts collapse <session> <thought> --collapsed <true|false> [--operation-id OP_ID]
proqi thoughts move <session> <thought> <position> [--operation-id OP_ID]
proqi thoughts send <source> <thought> <destination> [--remove] [--operation-id OP_ID] [--remove-operation-id OP_ID]
proqi thoughts undo <session> [--thought THOUGHT] [--operation-id OP_ID]
proqi thoughts redo <session> [--thought THOUGHT] [--operation-id OP_ID]
```

`add` and `replace` read the complete body from standard input:

```sh
printf '%s' 'Review the retry path.' | proqi --json thoughts add <session>
```

Positions are zero-based. List output preserves Board order and distinguishes
thoughts from payload-free separators. Inspect returns one exact thought body
plus metadata. Names remain separate metadata.

Replace normally requires the SHA-256 digest of current content so a stale
writer cannot overwrite newer text. `--force` is an explicit opt-out. External
replacement becomes an editor revision and participates in normal undo.

Send copies one thought into another Proqi session. `--remove` removes the
source only after destination durability. When supplied, operation and revision
identities make matching retries idempotent and reject divergent reuse.

## Check updates

```sh
proqi update check
```

This explicitly queries the verified installable stable channel. It does not
install anything. Automatic interactive startup checks can be disabled without
disabling this command.

## Diagnose without exposing content

```text
proqi doctor
proqi diagnostics collect [--output PATH]
proqi diagnostics keypress [--context CONTEXT,...] [--timeout-ms 100..60000] [--defaults]
```

`doctor` performs read-only local health checks and never repairs state.
Diagnostics collection writes a new bounded, content-redacted local file and
never uploads or overwrites. Review it before sharing.

Keypress captures one logical key and reports its modifiers, phase, selected
context, and configured action. Escape cancels. `--defaults` ignores user
configuration, including an invalid file. A timeout means no event reached
Proqi; it does not identify which upstream layer consumed it.

See [Privacy and diagnostics](privacy-and-diagnostics.md).

## Install the shipped agent skills

The optional Proqi skill teaches compatible coding agents to discover the
installed CLI contract, use JSON and standard input, and address an explicit
session without reading SQLite or scraping the TUI:

```sh
npx skills add oborchers/proqi --skill proqi -g --agent codex --agent claude-code
```

For read-only-first failure investigation:

```sh
npx skills add oborchers/proqi --skill proqi-debug -g
```

These skills do not install the Proqi executable. They describe the shipped
v0.11.0 contract only; automation should still begin with
`proqi --json capabilities`.
