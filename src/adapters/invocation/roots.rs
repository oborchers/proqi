use std::path::{Path, PathBuf};

use crate::ports::invocation::{InvocationHarness, InvocationKind, InvocationScope};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RootShape {
    Skills,
    MarkdownCommands,
    MarkdownAgents,
    TomlAgents,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CompatibilityRoot {
    pub(super) relative: &'static str,
    pub(super) scope: InvocationScope,
    pub(super) harness: InvocationHarness,
    pub(super) kind: InvocationKind,
    pub(super) shape: RootShape,
    pub(super) precedence: u16,
    pub(super) insertable: bool,
}

/// Evidence-backed compatibility roots kept intentionally small and reviewable.
pub(super) const COMPATIBILITY_ROOTS: &[CompatibilityRoot] = &[
    project(
        ".agents/skills",
        InvocationHarness::AgentSkills,
        InvocationKind::Skill,
        RootShape::Skills,
        20,
    ),
    project(
        ".claude/skills",
        InvocationHarness::ClaudeCode,
        InvocationKind::Skill,
        RootShape::Skills,
        25,
    ),
    project(
        ".claude/commands",
        InvocationHarness::ClaudeCode,
        InvocationKind::Command,
        RootShape::MarkdownCommands,
        25,
    ),
    project(
        ".claude/agents",
        InvocationHarness::ClaudeCode,
        InvocationKind::Agent,
        RootShape::MarkdownAgents,
        15,
    ),
    project(
        ".codex/agents",
        InvocationHarness::Codex,
        InvocationKind::Agent,
        RootShape::TomlAgents,
        20,
    ),
    project(
        ".opencode/commands",
        InvocationHarness::OpenCode,
        InvocationKind::Command,
        RootShape::MarkdownCommands,
        10,
    ),
    project(
        ".opencode/agents",
        InvocationHarness::OpenCode,
        InvocationKind::Agent,
        RootShape::MarkdownAgents,
        10,
    ),
    project(
        ".pi/skills",
        InvocationHarness::Pi,
        InvocationKind::Skill,
        RootShape::Skills,
        20,
    ),
    project(
        ".pi/prompts",
        InvocationHarness::Pi,
        InvocationKind::Command,
        RootShape::MarkdownCommands,
        20,
    ),
    catalog_only_project(".continue/skills", 30),
    catalog_only_project(".goose/skills", 30),
    catalog_only_project(".windsurf/skills", 30),
    global(
        ".agents/skills",
        InvocationHarness::AgentSkills,
        InvocationKind::Skill,
        RootShape::Skills,
        40,
    ),
    global(
        ".config/agents/skills",
        InvocationHarness::AgentSkills,
        InvocationKind::Skill,
        RootShape::Skills,
        45,
    ),
    global(
        ".codex/skills",
        InvocationHarness::Codex,
        InvocationKind::Skill,
        RootShape::Skills,
        50,
    ),
    global(
        ".claude/skills",
        InvocationHarness::ClaudeCode,
        InvocationKind::Skill,
        RootShape::Skills,
        5,
    ),
    global(
        ".claude/commands",
        InvocationHarness::ClaudeCode,
        InvocationKind::Command,
        RootShape::MarkdownCommands,
        5,
    ),
    global(
        ".claude/agents",
        InvocationHarness::ClaudeCode,
        InvocationKind::Agent,
        RootShape::MarkdownAgents,
        35,
    ),
    global(
        ".codex/agents",
        InvocationHarness::Codex,
        InvocationKind::Agent,
        RootShape::TomlAgents,
        40,
    ),
    global(
        ".config/opencode/skills",
        InvocationHarness::OpenCode,
        InvocationKind::Skill,
        RootShape::Skills,
        40,
    ),
    global(
        ".config/opencode/commands",
        InvocationHarness::OpenCode,
        InvocationKind::Command,
        RootShape::MarkdownCommands,
        40,
    ),
    global(
        ".config/opencode/agents",
        InvocationHarness::OpenCode,
        InvocationKind::Agent,
        RootShape::MarkdownAgents,
        40,
    ),
    global(
        ".pi/agent/skills",
        InvocationHarness::Pi,
        InvocationKind::Skill,
        RootShape::Skills,
        40,
    ),
    global(
        ".pi/agent/prompts",
        InvocationHarness::Pi,
        InvocationKind::Command,
        RootShape::MarkdownCommands,
        40,
    ),
    catalog_only_global(".continue/skills", 55),
    catalog_only_global(".cursor/skills", 55),
    catalog_only_global(".gemini/skills", 55),
    catalog_only_global(".copilot/skills", 55),
    catalog_only_global(".config/goose/skills", 55),
    catalog_only_global(".codeium/windsurf/skills", 55),
];

const fn project(
    relative: &'static str,
    harness: InvocationHarness,
    kind: InvocationKind,
    shape: RootShape,
    precedence: u16,
) -> CompatibilityRoot {
    CompatibilityRoot {
        relative,
        scope: InvocationScope::Project,
        harness,
        kind,
        shape,
        precedence,
        insertable: true,
    }
}

const fn global(
    relative: &'static str,
    harness: InvocationHarness,
    kind: InvocationKind,
    shape: RootShape,
    precedence: u16,
) -> CompatibilityRoot {
    CompatibilityRoot {
        relative,
        scope: InvocationScope::Global,
        harness,
        kind,
        shape,
        precedence,
        insertable: true,
    }
}

const fn catalog_only_project(relative: &'static str, precedence: u16) -> CompatibilityRoot {
    CompatibilityRoot {
        relative,
        scope: InvocationScope::Project,
        harness: InvocationHarness::AgentSkills,
        kind: InvocationKind::Skill,
        shape: RootShape::Skills,
        precedence,
        insertable: false,
    }
}

const fn catalog_only_global(relative: &'static str, precedence: u16) -> CompatibilityRoot {
    CompatibilityRoot {
        relative,
        scope: InvocationScope::Global,
        harness: InvocationHarness::AgentSkills,
        kind: InvocationKind::Skill,
        shape: RootShape::Skills,
        precedence,
        insertable: false,
    }
}

pub(super) fn project_bases(cwd: &Path) -> Vec<PathBuf> {
    let mut bases = Vec::new();
    let mut current = cwd.to_path_buf();
    for _ in 0..16 {
        bases.push(current.clone());
        if current.join(".git").exists() || !current.pop() {
            break;
        }
    }
    if !bases.iter().any(|path| path.join(".git").exists()) {
        bases.truncate(1);
    }
    bases
}
