//! Terminal-independent catalog of authoring-time harness invocations.

use std::path::PathBuf;

use super::agent::{AgentFailureCode, AgentState, HarnessKind};

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
    /// Harness-specific root precedence used when ordering this form.
    pub precedence: u16,
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
    /// Lower values sort before lower-precedence definitions without forms.
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
    /// Recognized collaborators currently present on the local terminal server.
    pub live: Vec<LiveAgentReference>,
}

/// Integration that supplied one ephemeral collaborator reference.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum InvocationReferenceProvider {
    /// The current local Herdr server.
    Herdr,
}

impl InvocationReferenceProvider {
    /// Compact group label used in the invocation picker.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Herdr => "Live in Herdr",
        }
    }
}

/// One bounded ephemeral collaborator location safe for display and inert insertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveAgentReference {
    provider: InvocationReferenceProvider,
    agent_name: Option<String>,
    harness: HarnessKind,
    workspace_id: String,
    workspace_label: Option<String>,
    tab_id: String,
    tab_label: Option<String>,
    pane_id: String,
    state: AgentState,
}

impl LiveAgentReference {
    /// Construct a reference whose exact location remains safe and useful to insert.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "the live reference value validates independent identity and display fields"
    )]
    pub fn new(
        provider: InvocationReferenceProvider,
        agent_name: Option<String>,
        harness: HarnessKind,
        workspace_id: String,
        workspace_label: Option<String>,
        tab_id: String,
        tab_label: Option<String>,
        pane_id: String,
        state: AgentState,
    ) -> Option<Self> {
        if agent_name
            .as_deref()
            .is_some_and(|name| !bounded_label(name, 32))
            || !bounded_identity(harness.as_str(), 32)
            || !bounded_identity(&workspace_id, 64)
            || workspace_label
                .as_deref()
                .is_some_and(|label| !bounded_label(label, 48))
            || !bounded_identity(&tab_id, 64)
            || tab_label
                .as_deref()
                .is_some_and(|label| !bounded_label(label, 48))
            || !bounded_identity(&pane_id, 64)
        {
            return None;
        }
        Some(Self {
            provider,
            agent_name,
            harness,
            workspace_id,
            workspace_label,
            tab_id,
            tab_label,
            pane_id,
            state,
        })
    }

    /// Supplying integration.
    #[must_use]
    pub const fn provider(&self) -> InvocationReferenceProvider {
        self.provider
    }

    /// Optional bounded collaborator session name supplied by the harness.
    #[must_use]
    pub fn agent_name(&self) -> Option<&str> {
        self.agent_name.as_deref()
    }

    /// Recognized coding-agent harness.
    #[must_use]
    pub const fn harness(&self) -> &HarnessKind {
        &self.harness
    }

    /// Opaque workspace identity on the current server.
    #[must_use]
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// Optional bounded user-facing workspace label from the same snapshot.
    #[must_use]
    pub fn workspace_label(&self) -> Option<&str> {
        self.workspace_label.as_deref()
    }

    /// Opaque tab identity within the workspace.
    #[must_use]
    pub fn tab_id(&self) -> &str {
        &self.tab_id
    }

    /// Optional bounded user-facing tab label from the same snapshot.
    #[must_use]
    pub fn tab_label(&self) -> Option<&str> {
        self.tab_label.as_deref()
    }

    /// Opaque pane identity currently hosting the collaborator.
    #[must_use]
    pub fn pane_id(&self) -> &str {
        &self.pane_id
    }

    /// Ephemeral readiness rendered only in the live picker row.
    #[must_use]
    pub const fn state(&self) -> AgentState {
        self.state
    }
}

fn bounded_label(value: &str, maximum: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed == value
        && trimmed.chars().count() <= maximum
        && !trimmed.chars().any(char::is_control)
}

fn bounded_identity(value: &str, maximum: usize) -> bool {
    bounded_label(value, maximum) && !value.chars().any(char::is_whitespace)
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

/// Blocking ephemeral reference discovery composed into the invocation refresh.
pub trait InvocationReferenceCatalog: Send {
    /// Discover recognized coding agents without reading pane content or arbitrary shells.
    ///
    /// # Errors
    ///
    /// Returns a content-free provider classification. Callers may degrade to no live group.
    fn discover_live_references(&mut self) -> Result<Vec<LiveAgentReference>, AgentFailureCode>;
}
