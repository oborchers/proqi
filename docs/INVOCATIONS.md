# Invocation catalog compatibility

Proqi discovers local authoring definitions without executing them. The model
keeps Skill, Command, and Agent as distinct kinds and records harness, scope,
canonical path, and zero or more documented authoring forms. An empty form list
means the definition is useful catalog evidence but cannot be inserted.

| Harness / ecosystem | Kind | Project roots | Global roots | Plugin scope | Inserted form | Portability decision |
| --- | --- | --- | --- | --- | --- | --- |
| Agent Skills / npx skills | Skill | `.agents/skills` | `~/.agents/skills`, `~/.config/agents/skills` | Harness-specific | `$name` for the documented Codex form | `SKILL.md` metadata is portable; exact forms retain their receiving harness and a receiving harness still decides availability |
| OpenAI Codex | Skill | `.agents/skills` from cwd through repository root | `~/.agents/skills`; `~/.codex/skills` as npx-skills compatibility | Bundled/system skills are outside user configuration | `$name` | First-class Agent Skills support |
| OpenAI Codex | Agent | `.codex/agents/*.toml` | `~/.codex/agents/*.toml` | None documented | Catalog-only | Codex documents natural-language delegation and `/agent` thread management, not an exact per-agent token |
| Claude Code | Skill | `.claude/skills/**/SKILL.md` from cwd through repository root | `~/.claude/skills` | Installed plugin `skills/` | `/name`; plugin `/plugin:name` | Skill wins a same-name legacy command in Claude; Proqi preserves both typed records and orders documented precedence |
| Claude Code | Command | `.claude/commands/**/*.md` | `~/.claude/commands` | Manifest `commands` paths or plugin `commands/` | `/name`; plugin `/plugin:name` | Legacy commands remain documented and invokable |
| Claude Code | Agent | `.claude/agents/**/*.md` | `~/.claude/agents` | Manifest `agents` paths or plugin `agents/` | `@agent-name`; plugin `@agent-plugin:name` | Agents are never mislabeled as skills or slash commands |
| OpenCode | Skill | Shared `.agents/skills` | `~/.config/opencode/skills` | Package-managed roots require explicit configuration | Catalog-only | OpenCode's skill tool has no equivalent exact authored token |
| OpenCode | Command | `.opencode/commands/**/*.md` | `~/.config/opencode/commands` | Explicit configured roots | `/path/name` | Project definitions precede global definitions |
| OpenCode | Agent | `.opencode/agents/**/*.md` | `~/.config/opencode/agents` | Explicit configured roots | `@name` for subagent/all mode; primary-only definitions are catalog-only | Mode metadata controls whether insertion is truthful |
| Pi | Skill | `.pi/skills`, shared `.agents/skills` | `~/.pi/agent/skills` | Package roots require explicit configuration | `/skill:name` | The Pi setting may disable skill commands; Proqi offers documented syntax without claiming it is enabled |
| Pi | Command | `.pi/prompts/*.md` | `~/.pi/agent/prompts` | Package roots require explicit configuration | `/name` | Prompt templates are commands, not skills |
| Other npx-skills compatibility | Skill | `.continue/skills`, `.goose/skills`, `.windsurf/skills` | `~/.continue/skills`, `~/.cursor/skills`, `~/.gemini/skills`, `~/.copilot/skills`, `~/.config/goose/skills`, `~/.codeium/windsurf/skills` | Harness-specific | Catalog-only | Portable metadata is retained, but Proqi does not invent a cross-harness authoring token |

Precedence is deterministic and harness-specific: nearest project roots precede
farther project roots; project precedes global for Codex, OpenCode, and Pi;
Claude personal skills/commands precede project while Claude project agents
precede user agents; plugin definitions remain lowest. Canonical paths collapse
symlink aliases while same-name definitions with distinct type or source remain
separately labeled. When a `.claude/skills` entry resolves to the corresponding
physical `.agents/skills` definition, `.agents` remains the canonical owner and
the one catalog entry retains both `$name` and `/name` with independent harness
precedence. The physical definition may itself be reached through a symlinked
skill folder in `.agents/skills`; a Claude alias that explicitly points through
that Agent Skills entry retains both forms after canonical deduplication.
Independent aliases to the same external definition do not establish that
relationship, and the reverse symlink direction does not gain shared forms.
Copy-mode installations remain separate harness-specific entries because their
physical definitions can diverge even when names and metadata initially match.

## Target boundaries and shared built-ins

If verified adjacent targets map to documented harnesses, completion and
highlighting include only forms accepted by those harnesses. A Codex-only target
therefore does not offer `.claude` forms, a Claude-only target does not offer
Codex forms, and several known targets contribute their union. With no
recognized adjacent target, all documented forms remain available as a
scratchpad authoring fallback. Submission remains exact plain text rather than
a runtime validation or execution boundary.

The checked-in shared-command table supplies `/plan` and `/goal`, both
documented by Codex and Claude Code. Each appears as a shared Command result only
at byte zero of a thought when a verified adjacent target for either harness
exists. Their compact label is `Shared Command`; target detection controls
availability separately. Leading whitespace, later lines, partial names such as
`/planner`, and in-body starter prose do not match or highlight.

For a multi-thought submission to either supported harness, Proqi preserves a
complete `/plan` or `/goal` starter on the first thought and omits either shared
starter from later thoughts in the outbound prompt. It removes the token and one
following whitespace separator only. Source thoughts remain byte-for-byte
unchanged.

Exact compatible invocation tokens are detected with bounded, sigil-aware
ranges outside fenced code and receive the same annotation color plus bold cue
as folded image and large-paste placeholders. The styling is render-only: it
does not create durable annotations or change editor text, wrapping, cursor
positions, persistence, or undo.

The byte-zero rule applies only to the shared `/plan` and `/goal` starters.
Discovered compatible slash forms, including project and local skills or
commands, highlight at exact token boundaries after whitespace and on later
logical lines. Partial names, embedded paths, URLs, fenced code, unsupported
target forms, and non-boundary occurrences remain plain.

## Live Herdr references

In a Herdr-managed pane, the existing picker refreshes one bounded protocol 19
snapshot when it opens. Its `Live in Herdr` group contains only recognized
coding agents. Each row prefers an explicit agent name and composes the
workspace label, a distinct tab label, pane, nonduplicate harness, and observed
state into the existing quiet secondary field. Exact IDs replace unavailable
workspace or tab labels. Directories and terminal titles never supply labels.

Selection inserts plain text with the agent name plus exact workspace, tab, and
pane identities. Proqi displays that canonical range as an unbracketed inline
mention and preserves it through undo, redo, persistence, transfer, recovery,
copy, and submission. The observed state remains display-only and refreshes only
when the picker reopens. Selecting a reference never submits, focuses, reserves,
or otherwise mutates the target.

## Evidence and licenses

- [Claude Code skills and slash commands](https://code.claude.com/docs/en/slash-commands), [subagents](https://code.claude.com/docs/en/sub-agents), and [plugin manifests](https://code.claude.com/docs/en/plugins-reference) are vendor documentation.
- [OpenAI Codex skills](https://developers.openai.com/codex/skills/) and [subagents](https://developers.openai.com/codex/subagents/) are vendor documentation.
- [OpenAI Codex developer commands](https://learn.chatgpt.com/docs/developer-commands?surface=cli) documents `/plan`, `/goal`, and their CLI availability.
- [OpenCode commands](https://opencode.ai/docs/commands/) and [agents](https://opencode.ai/v2/docs/agents/) are vendor documentation.
- [Agent Skills specification](https://agentskills.io/specification) defines bounded `SKILL.md` metadata.
- [Vercel Labs `skills`](https://github.com/vercel-labs/skills) informed the compatibility-root table and is MIT licensed. No source code was copied.
- [Pi](https://github.com/earendil-works/pi) documentation informed prompt and skill roots and is MIT licensed. No source code was copied.
- The [OpenAI Codex](https://github.com/openai/codex) TUI's render-only mention highlighting informed the separation between exact text and styled ranges. The reviewed source was commit `8bcac28f93f78b70d1159d97dbf11254bfb56a49`, licensed Apache-2.0; no source code was copied.
