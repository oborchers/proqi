//! Terminal-independent catalog of authoring-time harness invocations.

use std::path::PathBuf;

/// Conceptual definition layer represented by a catalog entry.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationKind {
    /// An Agent Skills-compatible `SKILL.md` definition.
    Skill,
    /// A filesystem-defined user command or prompt template.
    Command,
    /// An agent or subagent definition.
    Agent,
}

impl InvocationKind {
    /// Compact label used in completion results.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Skill => "Skill",
            Self::Command => "Command",
            Self::Agent => "Agent",
        }
    }
}

/// Harness whose documented authoring surface owns an invocation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationHarness {
    /// Portable Agent Skills ecosystem and shared roots.
    AgentSkills,
    /// Codex.
    Codex,
    /// Anthropic Claude Code.
    ClaudeCode,
    /// `OpenCode`.
    OpenCode,
    /// Pi coding agent.
    Pi,
    /// User-configured format without an assumed runtime.
    Configured,
}

impl InvocationHarness {
    /// Human-readable source label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AgentSkills => "Agent Skills",
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
            Self::OpenCode => "OpenCode",
            Self::Pi => "Pi",
            Self::Configured => "Configured",
        }
    }
}

/// Filesystem scope that supplied a definition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationScope {
    /// Definition rooted at the current project or one of its ancestors.
    Project,
    /// Machine-global definition independent of the current directory.
    Global,
    /// Installed plugin component.
    Plugin,
}

impl InvocationScope {
    /// Compact label used in completion results.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Global => "Global",
            Self::Plugin => "Plugin",
        }
    }
}

/// One exact token a documented harness accepts in authored text.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InvocationForm {
    /// Owning harness.
    pub harness: InvocationHarness,
    /// Exact canonical token, including its `$`, `/`, or `@` sigil.
    pub token: String,
}

/// One bounded definition discovered from the local filesystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationEntry {
    /// Metadata name or documented filename-derived identifier.
    pub name: String,
    /// Sanitized single-line description, when present.
    pub description: Option<String>,
    /// Conceptual definition layer.
    pub kind: InvocationKind,
    /// Project, global, or plugin scope.
    pub scope: InvocationScope,
    /// Source harness or compatibility ecosystem.
    pub source: InvocationHarness,
    /// Evidence-backed authoring forms. Empty means catalog-only.
    pub forms: Vec<InvocationForm>,
    /// Canonical physical definition path used only for identity and precedence.
    pub canonical_path: PathBuf,
    /// Lower values sort before lower-precedence definitions.
    pub precedence: u16,
}

/// Monotonic refresh request used to reject stale asynchronous results.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationDiscoveryRequest {
    /// Request generation allocated by the UI owner.
    pub generation: u64,
    /// Current working directory defining project scope.
    pub cwd: PathBuf,
}

/// Successful bounded discovery result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationDiscovery {
    /// Matching request generation.
    pub generation: u64,
    /// Exact request cwd used to reject results from an older session directory.
    pub cwd: PathBuf,
    /// Independently refreshed machine-global entries.
    pub global: Vec<InvocationEntry>,
    /// Entries associated with this cwd's ancestor chain.
    pub project: Vec<InvocationEntry>,
}

/// Explicit additional compatibility root from user configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdditionalInvocationRoot {
    /// Absolute root, or a path resolved relative to the session cwd.
    pub path: PathBuf,
    /// Explicit definition kind; never inferred from the path.
    pub kind: InvocationKind,
    /// Explicit parser and invocation contract.
    pub harness: InvocationHarness,
    /// Whether the root follows cwd or remains machine-global.
    pub scope: InvocationScope,
}

/// Best-effort discovery failure. Individual invalid entries are skipped.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InvocationCatalogError {
    /// A requested root set exceeds the adapter's fixed safety budget.
    #[error("invocation root budget exceeded")]
    RootBudget,
}

/// Blocking catalog capability implemented outside the application lane.
pub trait InvocationCatalog: Send {
    /// Refresh project and global definitions for one cwd.
    ///
    /// # Errors
    ///
    /// Returns a bounded catalog error when the configured root budget is exceeded.
    fn discover(
        &mut self,
        request: InvocationDiscoveryRequest,
    ) -> Result<InvocationDiscovery, InvocationCatalogError>;
}
