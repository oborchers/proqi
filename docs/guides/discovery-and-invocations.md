# Discover commands, skills, and collaborators

<span class="version-scope">Proqi 0.11.0</span>

Proqi has two discovery systems. **Commands** finds Proqi actions. The inline
invocation picker finds local agent skills, slash commands, and recognized
collaborators while you edit a thought.

## Search all Proqi actions

Press `:` in Board mode. Commands initially shows a short **Relevant now**
set, then searches every searchable Commands entry as you type. Ranking is
deterministic and Unicode-aware.

Unavailable commands are filtered by the current context and state. For
example, **Extract selection as new thought** requires an editor selection, and
**Retry Screenshot Capture** requires a retained failed capture.

Use arrows to move, page keys or Alt plus vertical arrows to jump, `Enter` or a
click to run, and `Escape` to close. Printable `j` and `k` remain query text in
Commands. The Commands query has its own transient undo history and never falls
through to hidden Board history.

The [Commands reference](../reference/commands.md) lists every action.

## Use contextual Help

Press `?`. Help is projected from the same action registry as
Commands, keyboard dispatch, footer controls, and diagnostics. It shows only
the controls relevant to the active Board, editor, or recovery context and uses
effective configured bindings.

## Complete a local invocation

While editing, type a supported sigil followed by part of a name:

- `$` for discovered skills;
- `/` for discovered command or prompt definitions;
- supported `@` forms for recognized collaborators.

Exact and prefix matches rank first. Contiguous and separator-aware fuzzy
matches follow, so a compact ordered abbreviation can find a longer hyphenated
name. Sigils are hard namespaces: dollar, slash, and at results never mix.

Use arrows or `Primary+P` and `Primary+N` to navigate. Press `Enter` or `Tab` to
insert the selected invocation, or `Escape` to close. Results beyond
the visible viewport remain reachable by keyboard and mouse.

## Refresh discoveries

Commands exposes separate refresh actions for invocations and adjacent agents.
Use them after local definitions, plugins, packages, or Herdr topology change.
Refresh never edits the thought by itself.

## Understand collaborator mentions

Inside Herdr, the picker can discover recognized live coding agents on the same
server. Selecting one inserts an inert collaborator location and renders it as
a compact mention. It does not focus, message, or otherwise control that agent.

Actual prompt delivery remains a separate deliberate action. See
[Deliver prompts to agents](agent-delivery.md).
