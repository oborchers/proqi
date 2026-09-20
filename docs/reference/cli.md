# Command-line reference

<span class="version-scope">Proqi 0.11.0 plus next-release main</span>

!!! note "Version boundary"

    The published v0.11.0 binary does not include the `items` family or the
    `thoughts split`, `extract`, `merge`, and `reflow` operations. Those eight
    additive commands are available on `main` for the next release and are
    labeled below. Always read `capabilities` from the installed binary before
    using an operation.

Run `proqi --help` or append `--help` to any command for the installed contract.
Human output is for people. Add global `--json` when a script or coding agent
needs the versioned machine envelope.

CLI and JSON compatibility can change between minor releases before 1.0. Read
capabilities first and use standard input for thought bodies rather than shell
arguments.

## Common options

```text
proqi --help
proqi -h
proqi --version
proqi -V
proqi --json <command>
```

`-h` is the short form of `--help`, which is also available on every command
and subcommand. `-V` is the short form of `--version`, which prints the
installed release. `--json` is global, so it can precede or follow a command;
examples keep it immediately after `proqi` for consistency.

## Interactive startup

```text
proqi
proqi -c
proqi --continue
proqi -r [ID_OR_NAME]
proqi --resume [ID_OR_NAME]
```

`--continue` opens the latest inactive session ranked for the current working
directory. `-c` is its short form. `--resume` without an argument opens the
Session Browser, and `-r` is its short form. An explicit reference accepts one
canonical ID or unique session name.

## Discover capabilities

```sh
proqi --json capabilities
```

The response reports the current schema, identifier encoding, input bounds,
control protocol, transfer support, update support, and optional Herdr
capabilities. In v0.11.0, `commands` names the `sessions`, `thoughts`, and
`update` families. Next-release main expands that list to every top-level
family, adds an `operations` inventory with the separate Board, editor, and
Browser history scopes, and makes the active-session flags platform-aware.
Read only fields that the installed response actually contains.

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
proqi sessions
proqi sessions list [-q, --query TEXT] [--all]
proqi sessions rename <session> (NAME | --clear)
proqi sessions trash <session>
proqi sessions restore <session>
proqi sessions undo
proqi sessions redo
proqi sessions prune <session> --yes
```

With no subcommand, `sessions` lists resumable sessions. Ranking prefers the
current directory. Search covers optional names, launch paths, and thought
content. `-q` is the short form of `--query`. `--all` includes recoverably
trashed sessions.

Rename, trash, and restore enter persistent Browser history. `sessions undo`
and `sessions redo` move that history. Prune is different: it permanently
deletes an already trashed session and requires `--yes`. It is not undoable.

## Change Board items

<span class="version-scope">Next release</span>

Use the `items` family when the target can be either a thought or a separator:

```text
proqi --json items insert-separator <session> [--position N] [--operation-id OP_ID]
proqi --json items move <session> <item> <position> [--operation-id OP_ID]
proqi --json items delete <session> <item>... [--operation-id OP_ID]
proqi --json items duplicate <session> <item>... [--operation-id OP_ID]
```

Positions are zero-based in the shared Board order. Omitting the insert
position appends the separator. Move accepts one typed `tht_` or `sep_`
identifier. Delete and duplicate accept one or more typed identifiers and apply
them as one atomic Board operation in visible order.

A separator has an identity and position but no content, annotations, or name.
It therefore never becomes prompt payload. Item deletion is recoverable through
Board undo. Duplication creates fresh typed identities.

Supplying a fresh `--operation-id` makes a mutation safely retryable. Repeating
the same request returns its original receipt with `idempotent_replay: true`.
Reusing that identity for different arguments fails with
`idempotency_conflict` rather than applying another mutation.

## Inspect and change thoughts

```text
proqi thoughts list <session>
proqi thoughts inspect <session> <thought>
proqi thoughts add <session> [--position N] [--operation-id OP_ID]
proqi thoughts delete <session> <thought> [--operation-id OP_ID]
proqi thoughts rename <session> <thought> (NAME | --clear) [--operation-id OP_ID]
proqi thoughts replace <session> <thought> (--expected-sha256 HEX | --force) [--revision-id REV_ID]
proqi thoughts collapse <session> <thought> --collapsed <true|false> [--operation-id OP_ID]
proqi thoughts move <session> <thought> <position> [--operation-id OP_ID]
proqi thoughts split <session> <thought> <at-byte> --expected-sha256 HEX [--operation-id OP_ID]
proqi thoughts extract <session> <thought> <start-byte> <end-byte> --expected-sha256 HEX [--operation-id OP_ID]
proqi thoughts merge <session> <thought> <thought>... --expected-sha256 HEX --expected-sha256 HEX... [--operation-id OP_ID]
proqi thoughts reflow <session> <thought> --expected-sha256 HEX [--operation-id OP_ID]
proqi thoughts send <source> <thought> <destination> [--remove] [--operation-id OP_ID] [--remove-operation-id OP_ID]
proqi thoughts undo <session> [--thought THOUGHT] [--operation-id OP_ID]
proqi thoughts redo <session> [--thought THOUGHT] [--operation-id OP_ID]
```

`add` and `replace` read the complete body from standard input:

```sh
printf '%s' 'Review the retry path.' | proqi --json thoughts add <session>
```

Positions are zero-based. List output preserves Board order in `items`, retains
the legacy ordered `thoughts` view, and distinguishes thoughts from payload-free
separators. Inspect returns one exact thought body, its `content_sha256`, and
metadata. Names remain separate metadata.

Rename requires a name or `--clear`. An empty thought name also clears it.

Replace normally requires the SHA-256 digest of current content so a stale
writer cannot overwrite newer text. `--force` is an explicit opt-out. External
replacement becomes an editor revision and participates in normal undo.

The next four transformations are available on `main` for the next release.

Split and extract use zero-based UTF-8 byte offsets. Each offset must be a valid
character boundary in the exact inspected content. Extract uses a nonempty
half-open range from `start-byte` through, but not including, `end-byte`.

Merge requires at least two thoughts. Supply their identifiers and one
`--expected-sha256` value per thought in the same order. They must be contiguous
in the mixed Board order, so an intervening separator rejects the request. The
first thought keeps its identity and name, the others are recoverably deleted,
and their exact bodies are joined with the configured `merge_separator`.

Reflow applies the same annotation-safe canonical spacing policy as **Clean up
spacing**. An unchanged body fails with `no_change` and creates no history entry.
Every transformation checks current content before changing it, commits as one
Board operation, and participates in Board undo and redo.

Send copies one thought into another Proqi session. `--remove` removes the
source only after destination durability. When supplied, operation and revision
identities make matching retries idempotent and reject divergent reuse.
`--remove-operation-id` is accepted only together with `--remove`.

On macOS and Linux, a supported mutation aimed at an active session is sent to
that session's authoritative process. Reads synchronize first. On next-release
main, other platforms report active control as unavailable in `capabilities`;
they do not bypass the session lease. The CLI does not expose TUI focus, cursor
or pointer geometry, clipboard acquisition, host target discovery, or raw key
injection.

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

These skills do not install the Proqi executable. Their supported operations
follow the repository version that supplies them, so automation must still
begin with `proqi --json capabilities` from the installed executable.
