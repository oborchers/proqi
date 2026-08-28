# Invocation catalog compatibility

Proqi discovers local authoring definitions without executing them. The model
keeps Skill, Command, and Agent as distinct kinds and records harness, scope,
canonical path, and zero or more documented authoring forms. An empty form list
means the definition is useful catalog evidence but cannot be inserted.

| Harness / ecosystem | Kind | Project roots | Global roots | Plugin scope | Inserted form | Portability decision |
| --- | --- | --- | --- | --- | --- | --- |
| Agent Skills / npx skills | Skill | `.agents/skills` | `~/.agents/skills`, `~/.config/agents/skills` | Harness-specific | `$name` where supported | `SKILL.md` metadata is portable; a receiving harness still decides availability |
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
separately labeled.

## Evidence and licenses

- [Claude Code skills and slash commands](https://code.claude.com/docs/en/slash-commands), [subagents](https://code.claude.com/docs/en/sub-agents), and [plugin manifests](https://code.claude.com/docs/en/plugins-reference) are vendor documentation.
- [OpenAI Codex skills](https://developers.openai.com/codex/skills/) and [subagents](https://developers.openai.com/codex/subagents/) are vendor documentation.
- [OpenCode commands](https://opencode.ai/docs/commands/) and [agents](https://opencode.ai/v2/docs/agents/) are vendor documentation.
- [Agent Skills specification](https://agentskills.io/specification) defines bounded `SKILL.md` metadata.
- [Vercel Labs `skills`](https://github.com/vercel-labs/skills) informed the compatibility-root table and is MIT licensed. No source code was copied.
- [Pi](https://github.com/earendil-works/pi) documentation informed prompt and skill roots and is MIT licensed. No source code was copied.
