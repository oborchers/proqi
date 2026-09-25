# Command-line reference

<span class="version-scope">Proqi 0.14.0</span>

!!! note "Version history"

    Proqi 0.12.0 added the `items` family and the `thoughts split`, `extract`,
    `merge`, and `reflow` operations. Proqi 0.13.0 added named session creation,
    retry identities for session mutations, named thought creation, bounded
    lists, and JSON help and version output. It also removed the legacy
    `thoughts` array from `thoughts list`. Proqi 0.14.0 adds `herdr toggle`.
    Always read `capabilities` from the installed binary before using an
    operation.

Run `proqi --help` or append `--help` to any command for the installed contract.
Human output is for people. Add global `--json` when a script or coding agent
needs the versioned machine envelope.

Read capabilities first and use standard input for thought bodies rather than
shell arguments.

## Stability

The command-line contract has two layers. From 1.0, the machine layer is
stable:

- command and option spellings, positional arguments, and standard input
  semantics;
- exit statuses;
- the versioned `--json` envelope (`ok`, `data`, `error`, `schema_version`),
  its documented fields, and receipt semantics such as `idempotent_replay`;
- every error code with its exit status, retry class, and `details` shape, as
  listed in [Errors](#errors).

Within `schema_version` 1, Proqi only adds: new commands, options, fields, and
error codes may appear, and consumers must ignore fields they do not know.
Removing or renaming any of these, or changing what an existing field or code
means, requires a new schema version or an announced deprecation period.
`capabilities` reports which operations, options, and codes the installed binary
supports.

Human output is for people and is not a contract. Its wording, layout, and
ordering can change in any release, so scripts and agents must use `--json`.
The owner-control protocol between Proqi processes and the SQLite schema are
internal as well.

Before 1.0, CLI and JSON compatibility can still change between minor releases.
Each such change is announced in the release notes.

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

<span class="version-scope">Proqi 0.13.0</span>

With `--json`, help and version are successful informational requests. They
exit 0 and return `ok: true`. Version data is `{"name": "proqi", "version":
"X.Y.Z"}`, and help data is `{"help": "<rendered help text>"}`. A literal
`--json` after the `--` argument terminator is a positional value and never
selects JSON output.

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
capabilities. Since 0.13.0, `commands` names every top-level family, and
`operations` distinguishes Board, editor, and Browser history scopes. The
active-session flags are platform-aware.
Read only fields that the installed response actually contains.

<span class="version-scope">Proqi 0.13.0 and 0.14.0</span>

These releases add the following discovery fields:

- `options` lists the long options that each `sessions`, `items`, and
  `thoughts` operation accepts, derived from the installed parser. For example,
  `options.sessions.create` is `["name", "cwd", "operation-id"]`. Global options
  such as `--json` are omitted.
- `error_codes` lists every JSON error code with its exit status and retry class
  (`no`, `after_change`, `yes`, or `same_identity`), as documented in
  [Errors](#errors).
- Six semantic flags describe behavior that option names cannot show:
  `atomic_named_sessions` (`sessions ensure` and `sessions create`),
  `named_thought_creation` (`thoughts add --name`),
  `session_operation_identity` (`--operation-id` on session mutations),
  `idempotent_session_trash` (repeated trash succeeds),
  `bounded_lists` (`--limit`, `--after`, `total`, and `next_after`), and
  `json_help_and_version` (successful JSON help and version output).
- `herdr_companion_toggle` reports `proqi herdr toggle`, and
  `operations.herdr` lists `toggle`. The Herdr plugin's launcher checks this
  flag before it runs the toggle.

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
proqi sessions list [-q, --query TEXT] [--all] [--limit N] [--after SESSION_ID]
proqi sessions ensure --name NAME --cwd PATH
proqi sessions create --name NAME [--cwd PATH] [--operation-id OP_ID]
proqi sessions rename <session> (NAME | --clear) [--operation-id OP_ID]
proqi sessions trash <session> [--operation-id OP_ID]
proqi sessions restore <session> [--operation-id OP_ID]
proqi sessions undo [--operation-id OP_ID]
proqi sessions redo [--operation-id OP_ID]
proqi sessions prune <session> --yes [--operation-id OP_ID]
```

With no subcommand, `sessions` lists resumable sessions. Ranking prefers the
current directory. Search covers optional names, launch paths, and thought
content. `-q` is the short form of `--query`. `--all` includes recoverably
trashed sessions.

Rename, trash, and restore enter persistent Browser history. `sessions undo`
and `sessions redo` move that history. Prune is different: it permanently
deletes an already trashed session and requires `--yes`. It is not undoable.

### Create named sessions

<span class="version-scope">Proqi 0.13.0</span>

`sessions ensure` returns the one live session whose exact name and origin
directory match, and creates it when no live session uses the name. The name
lookup and the creation run in one storage transaction, so repeated and
concurrent calls with the same inputs return one session. The session receives
its name in the same commit that creates it, so no unnamed intermediate session
can remain after a failure.

`--cwd` must name an existing directory. Proqi resolves it the same way it
records an interactive launch directory, so symlinked spellings of one
directory match. Trashed sessions are ignored. When several live sessions match
the name and directory, the command fails with `ambiguous_session` and lists
them in `details.matches`. When the name belongs only to live sessions from
other directories, it fails with `session_name_conflict` and lists each
conflicting `id` and `origin_cwd`.

`sessions create` always creates one additional session, even when the name is
already in use. `--cwd` defaults to the current directory. Its session
identifier derives from the operation identity, so repeating the request with
the same `--operation-id` returns the same session instead of creating another.

Neither command opens a terminal interface or contacts Herdr. Both return:

```json
{
  "session_id": "ses_...",
  "name": "agent-os-claude",
  "origin_cwd": "/path/to/agent-os",
  "state": "resumable",
  "disposition": "created",
  "resume_command": "proqi -r ses_..."
}
```

`disposition` is `created` or `reused`. `state` uses the same values as
`sessions list`. `sessions create` additionally returns
`receipt: {session_id, operation_id, idempotent_replay}`. Open the returned
session with `proqi --resume <session-id>`.

### Retry session changes

<span class="version-scope">Proqi 0.13.0</span>

Every session mutation except `sessions ensure` accepts `--operation-id`.
`ensure` needs none, because repeating it with the same name and directory
already returns the same session. A retry with the same
identity and the same request returns the original result with
`idempotent_replay: true` instead of applying anything again. Reusing the
identity for another request, including a thought or item mutation, fails with
`idempotency_conflict`. Undo and redo requests match on their direction.

Rename, trash, restore, and prune return
`{session_id, status, changed, receipt: {session_id, operation_id,
idempotent_replay}}`. `changed` reports whether this call changed durable state, so an exact replay
always reports `changed: false` even when the original call changed it.
Trashing an already trashed session succeeds with `changed: false`. A rename to
the current name also reports `changed: false`.

A rename of a session that is open in an interactive Proqi is applied by that
process. The operation identity still applies it at most once. Every call after
the first reports `idempotent_replay: true` and `changed: false`, including
calls that overlap the first. An owner from an older release does not report
replays, so an overlapping duplicate sent to it can report
`idempotent_replay: false`. Undo and redo return
`{history, operation, cursor, receipt: {operation_id, idempotent_replay}}`.
When the moved entry's session was pruned afterward, a replay reports
`operation: null`, and the moved entry's identity stays reserved.

Prune retains its receipt after the session is deleted, so an exact retry
succeeds as a replay. Pruning forgets the session's rename, trash, and restore
receipts, as it forgets its thought and item receipts, so those identities can
be used again afterward. It retains a content-free creation receipt and every
undo and redo receipt, so those retries still replay. Retrying
the `sessions create` that made a pruned session therefore fails with
`session_not_found` and never recreates it.

A retry can address the session by the name it had before the request, for
example after `sessions rename old new` or `sessions prune old --yes`. When the
operation identity already names a session and the name no longer resolves, or
resolves ambiguously among sessions that include the recorded one, the recorded
session is used. When the name currently resolves to a different session, the
request fails with `idempotency_conflict` and changes nothing, so an
operation identity never acts on a session other than the named one. A typed
`ses_` identifier always addresses exactly that session.

A `sessions create` retry recomputes its request identity from the name and the
canonical `--cwd`, so that directory must still exist when the command is
retried.

A typed `ses_` identifier is accepted without a lookup, so an absent session
addressed that way fails later with `not_found`. A name that matches no session
fails with `session_not_found`, as does a creation replay whose session was
pruned.

## Change Board items

<span class="version-scope">Proqi 0.12.0</span>

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
proqi thoughts list <session> [--limit N] [--after ITEM_ID]
proqi thoughts inspect <session> <thought>
proqi thoughts add <session> [--name NAME] [--position N] [--operation-id OP_ID]
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

Positions are zero-based. List output preserves Board order in `items` and
distinguishes thoughts from payload-free separators. Each thought entry carries
`kind: "thought"`, `id`, `position`, `content`, `name`, `collapsed`,
`presentation`, `updated_at`, and `content_sha256`. Separator entries carry
`kind: "separator"`, `id`, `position`, `created_at`, and `updated_at`. Inspect
returns one exact thought body, its `content_sha256`, and metadata. Names
remain separate metadata.

<span class="version-scope">Proqi 0.13.0</span>

The legacy `thoughts` array is removed from `thoughts list`. Read thought
content from the `items` entries whose `kind` is `thought`.

`add --name` creates the content, name, and position as one Board operation and
one undo step, for active and inactive sessions. The name follows thought-name
rules: it must be nonblank, single-line, and at most 80 characters after
surrounding whitespace is trimmed. An invalid name fails with `invalid_input`
before any write. The name is part of the operation identity, so the same
`--operation-id` with a different name fails with `idempotency_conflict`.

### Bounded lists

<span class="version-scope">Proqi 0.13.0</span>

`thoughts list` and `sessions list` accept `--limit N`, where `N` is at least 1.
Both return `total`, the number of entries in the complete list, and
`next_after`, the identifier of the last returned entry when more remain, or
`null`. Pass `next_after` as `--after` to read the following page. A thought
list page contains Board items, including separators, so its `--after` accepts a
`tht_` or `sep_` identifier. A session list page accepts a `ses_` identifier and
follows the same ranking and filters as the first page. An `--after` entry that
is no longer listed fails with `cursor_not_found` rather than restarting. Pages
read the current state on each call, so concurrent changes can move entries
between pages. Human output lists the same page, shows separators as
`(separator)` rows, and ends with `Showing N of TOTAL. Continue with --after ID`
when more entries remain.

Rename requires a name or `--clear`. An empty thought name also clears it. A
name that breaks the thought-name rules fails with `invalid_input`, as it does
for `add --name`.

Replace normally requires the SHA-256 digest of current content so a stale
writer cannot overwrite newer text. `--force` is an explicit opt-out. External
replacement becomes an editor revision and participates in normal undo.

The following four transformations have been available since Proqi 0.12.0.

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
that session's authoritative process. Reads synchronize first. Other platforms
report active control as unavailable in `capabilities`;
they do not bypass the session lease. The CLI does not expose TUI focus, cursor
or pointer geometry, clipboard acquisition, host target discovery, or raw key
injection.

## Errors

<span class="version-scope">Proqi 0.13.0 and 0.14.0</span>

The complete table, `capabilities.error_codes`, and the codes
`session_name_conflict`, `cursor_not_found`, `clipboard_failed`, and
`clipboard_metadata_unsupported` were added in 0.13.0. An active owner
whose advertised control protocol cannot represent a request now reports
`protocol_mismatch` instead of the retryable `session_busy`. A lease holder
that advertises no protocol yet, such as another command in progress, still
reports `session_busy`. Proqi 0.14.0 adds `companion_session_active`,
`herdr_failed`, and `plugin_state_failed` for the Herdr plugin toggle.

With `--json`, every failure writes
`{"schema_version": 1, "ok": false, "error": {"code", "message", "details"}}` to
standard output and exits with the status below. `message` is human text and can
change. Branch on `code`. `capabilities` publishes the same inventory in
`error_codes`.

Exit status 0 is success, 2 is an invalid request, 3 is absent state, 4 is an
ambiguous reference, 5 is contention, 6 is an unsupported version or protocol,
7 is a state or precondition conflict, 8 is an indeterminate outcome, and 1 is
any other failure.

**Retry** states whether repeating the identical request can succeed:

- **No**: change the arguments or input first.
- **After change**: retry after the reported state changes, for example after
  restoring a session, re-inspecting content, or fixing the environment.
- **Yes**: retry after contention or a transient fault clears.
- **Same identity**: completion is unknown. Retry only with the same operation
  identity so the original result can be matched.

| Code | Exit | Retry | `details` |
|---|---|---|---|
| `invalid_arguments` | 2 | No | `{}` |
| `invalid_input` | 2 | No | `{}` |
| `invalid_identifier` | 2 | No | `{}` |
| `config_invalid` | 2 | No | `{}` |
| `invalid_shortcut_context` | 2 | No | `{}` |
| `unsafe_state_path` | 2 | No | `{}` |
| `session_not_found` | 3 | After change | `{}` |
| `thought_not_found` | 3 | After change | `{}` |
| `not_found` | 3 | After change | `{}` |
| `cursor_not_found` | 3 | After change | `{}` |
| `ambiguous_session` | 4 | After change | `{"matches": [session_id]}` |
| `session_busy` | 5 | Yes | `{}`, or `{"session_id", "holder"}` when the active owner is known |
| `companion_session_active` | 5 | After change | `{"session_id", "name"}`; `name` is null when a recorded session was reopened |
| `schema_busy` | 5 | Yes | `{}` |
| `storage_busy` | 5 | Yes | `{}` |
| `unsupported` | 6 | No | `{}` |
| `protocol_mismatch` | 6 | No | `{}`, or `{"session_id", "holder"}` when the active owner cannot represent the request |
| `clipboard_metadata_unsupported` | 6 | No | `{}` |
| `session_trashed` | 7 | After change | `{}` |
| `session_not_trashed` | 7 | After change | `{}` |
| `session_name_conflict` | 7 | After change | `{"name", "sessions": [{"id", "origin_cwd"}]}` |
| `history_unavailable` | 7 | After change | `{}` |
| `idempotency_conflict` | 7 | No | `{}` |
| `no_change` | 7 | No | `{}` |
| `content_conflict` | 7 | After change | `{}` |
| `thought_locked` | 7 | After change | `{}` |
| `invalid_state` | 7 | After change | `{}` |
| `invariant_violation` | 7 | No | `{}` |
| `conflict` | 7 | After change | `{}` |
| `mutation_rejected` | 7 | After change | `{}` |
| `operation_indeterminate` | 8 | Same identity | `{"session_id", "holder"}` |
| `storage_failed` | 1 | After change | `{}` |
| `storage_full` | 1 | After change | `{}` |
| `disk_full` | 1 | After change | `{}` |
| `recovery_capacity` | 1 | After change | `{}` |
| `runtime_failed` | 1 | After change | `{}` |
| `runtime_metadata_invalid` | 1 | After change | `{}` |
| `terminal_failed` | 1 | After change | `{}` |
| `terminal_worker_failed` | 1 | After change | `{}` |
| `terminal_cleanup_failed` | 1 | After change | `{}` |
| `control_failed` | 1 | After change | `{}` |
| `herdr_failed` | 1 | After change | `{}` |
| `plugin_state_failed` | 1 | Yes | `{}` |
| `output_failed` | 1 | After change | `{}` |
| `clipboard_failed` | 1 | After change | `{}` |
| `environment_failed` | 1 | After change | `{}` |
| `diagnostics_failed` | 1 | After change | `{}` |
| `doctor_failed` | 1 | After change | The complete `doctor` report |
| `installation_failed` | 1 | After change | `{}` |
| `installation_unverified` | 1 | After change | `{}` |
| `installed_version_invalid` | 1 | No | `{}` |
| `invalid_build_version` | 1 | No | `{}` |
| `obsolete_executable` | 1 | No | `{"current_version", "observed_installed_version"}` |
| `update_convergence_active` | 1 | Yes | `{}` |
| `external_upgrade_pending` | 1 | After change | `{"current_version", "pending_target_version", "sessions": [{"session_id", "previous_version"}]}` |
| `external_upgrade_blocked` | 1 | After change | `{"blockers": [blocker]}` |
| `external_upgrade_capacity` | 1 | After change | `{"blockers": [blocker], "participant_count", "maximum", "blockers_truncated"}` |
| `external_upgrade_incomplete` | 1 | After change | `{"blockers": [blocker]}` |
| `external_upgrade_persistence_failed` | 1 | After change | `{"blockers": [blocker]}` |
| `update_network_failed` | 1 | Yes | `{}` |
| `update_response_invalid` | 1 | After change | `{}` |
| `update_response_too_large` | 1 | After change | `{}` |
| `update_state_failed` | 1 | After change | `{}` |
| `update_coordination_failed` | 1 | After change | `{}` |
| `update_installation_failed` | 1 | After change | `{}` |

## Toggle Proqi beside a Herdr agent

<span class="version-scope">Proqi 0.14.0</span>

```text
proqi herdr toggle
```

This command is the action of the [Herdr plugin](../guides/herdr-plugin.md).
Herdr runs it with the plugin environment (`HERDR_ENV=1`, `HERDR_PLUGIN_ID`,
`HERDR_PLUGIN_CONTEXT_JSON`, and `HERDR_PLUGIN_STATE_DIR`). Anywhere else it
fails with `unsupported` and changes nothing.

It acts only on the tab that had focus:

- The plugin records one session per tab in its private state. When the tab
  has a recorded session, the toggle reopens it in a new pane to the right of
  the focused pane, whichever pane is focused.
- For a tab's first companion, it runs the same get-or-create as
  `sessions ensure`. The name is the Herdr name of the tab's agent when exactly
  one agent in the tab has a name. Otherwise it is the tab label; a numeric
  default label, which Herdr derives from the tab's position, is replaced by
  the stable tab identity, so tab `w1:t4` in workspace `demo` uses
  `demo-w1-t4`. When Herdr cannot list the tab's agents, the toggle fails with
  `herdr_failed` and opens nothing. The origin is the
  Herdr worktree checkout, else the Git repository root containing the focused
  pane's directory, else that directory.
- When the tab already shows a Proqi pane, it focuses that pane.
- When the focused pane is the Proqi pane the plugin opened, it asks that Proqi
  to make pending edits durable and closes the pane only after Proqi confirms.
  Without that confirmation, for example while Proqi is still starting, it fails
  with `session_busy` and keeps the pane. The session stays recorded.
- When the focused pane runs a Proqi that the plugin did not open, or a recorded
  pane that Herdr cannot classify in time, it returns focus to the tab's only
  agent pane and never closes that pane.
- When a recorded Proqi pane survived a Herdr restart as an idle shell, it
  opens the same session in a new pane and then closes the old pane only if it
  is still an idle shell. A recorded pane that now runs anything else is never
  touched.

When the session is already open in another pane, for example because another
workspace has a tab with the same label, or an agent with the same name, in the
same repository, the toggle fails with `companion_session_active` instead of
starting a second Proqi. When the derived name already belongs to a session
from another directory, for example two repositories that both have a tab
labeled `main`, it fails with `session_name_conflict`; rename the tab or agent
that named the session, or the other session. Failures are also
shown as a Herdr notification.

A successful JSON response has one of these shapes:

```json
{"action": "opened", "tab_id": "w1:t1", "pane_id": "w1:p3", "session_id": "ses_...", "replaced_pane_id": null}
{"action": "focused", "pane_id": "w1:p3"}
{"action": "returned", "pane_id": "w1:p1"}
{"action": "closed", "pane_id": "w1:p3", "session_id": "ses_..."}
```

Exit status 0 is success. Other statuses follow [Errors](#errors):
`unsupported` (6) outside the plugin, `invalid_input` (2) when the focused
pane's directory no longer exists, `ambiguous_session` (4) when several live
sessions share the tab's name and directory, `companion_session_active` or
`session_busy` (5), `session_name_conflict` or `invalid_state` (7),
`herdr_failed` (1) when Herdr rejects or cannot answer a request, including a
pane whose process Herdr cannot report in time, which blocks opening because it
might hide a Proqi, and
`plugin_state_failed` (1) when the plugin's private state or its toggle lock is
unavailable.

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

### Install as a Claude Code plugin

Claude Code users can install both skills from the repository's
[plugin marketplace](https://code.claude.com/docs/en/discover-plugins) instead
of `npx skills add`:

```text
/plugin marketplace add oborchers/proqi
/plugin install proqi@proqi
```

The equivalent shell commands are
`claude plugin marketplace add oborchers/proqi` and
`claude plugin install proqi@proqi`. Use one installation method per harness so
the same skill is not listed twice.

The `proqi` plugin contains exactly the `skills/proqi` and `skills/proqi-debug`
files that `npx skills add` installs, without repository-internal maintainer
skills. Claude Code namespaces plugin skills, so they appear as `/proqi:proqi`
and `/proqi:proqi-debug`; the bare `/proqi` form also works when no other skill
uses that name.

The marketplace serves the default branch, and the plugin version equals the
Cargo version. Claude Code therefore offers an update when a release changes
that version, not for every commit. Automatic updates are off by default for
third-party marketplaces. Refresh the catalog, then update the installed
plugin:

```text
/plugin marketplace update proqi
/plugin update proqi@proqi
```

Between release preparation and publication, the default branch can describe
operations that the installed executable does not yet provide. The skills
therefore require each operation's exact spelling in
`proqi --json capabilities` before using it.
