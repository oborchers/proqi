//! Closed verified addresses for semantic agent delivery.

use serde::{Deserialize, Serialize};

use crate::domain::Direction;

use super::{AgentSessionBinding, HarnessKind, PaneContext, PaneRect};

/// Current-server identity of one Herdr coding agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HerdrAgentAddress {
    /// Opaque workspace identity from the current server snapshot.
    workspace_id: String,
    /// Opaque tab identity from the same snapshot.
    tab_id: String,
    /// Opaque pane identity currently hosting the agent.
    pane_id: String,
    /// Recognized coding-agent harness.
    agent_kind: HarnessKind,
    /// Stable harness session, or one explicitly qualified provisional binding.
    agent_session: AgentSessionBinding,
}

impl HerdrAgentAddress {
    /// Construct one complete current-server delivery address.
    #[must_use]
    pub fn new(
        workspace_id: String,
        tab_id: String,
        pane_id: String,
        agent_kind: HarnessKind,
        agent_session: AgentSessionBinding,
    ) -> Option<Self> {
        if workspace_id.trim().is_empty() || tab_id.trim().is_empty() || pane_id.trim().is_empty() {
            return None;
        }
        Some(Self {
            workspace_id,
            tab_id,
            pane_id,
            agent_kind,
            agent_session,
        })
    }

    /// Return the current-server workspace identity.
    #[must_use]
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// Return the current-server tab identity.
    #[must_use]
    pub fn tab_id(&self) -> &str {
        &self.tab_id
    }

    /// Return the target pane identity.
    #[must_use]
    pub fn pane_id(&self) -> &str {
        &self.pane_id
    }

    /// Return the recognized harness kind.
    #[must_use]
    pub const fn agent_kind(&self) -> &HarnessKind {
        &self.agent_kind
    }

    /// Return the established or qualified provisional harness session.
    #[must_use]
    pub const fn agent_session(&self) -> &AgentSessionBinding {
        &self.agent_session
    }

    pub(super) fn replace_agent_kind(&mut self, kind: HarnessKind) {
        self.agent_kind = kind;
    }

    pub(super) fn replace_agent_session(&mut self, session: AgentSessionBinding) {
        self.agent_session = session;
    }
}

/// Stable closed route classification shared by persistence and diagnostics.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionRouteKind {
    /// Same-tab directional delivery with verified geometry.
    AdjacentPane,
    /// Current-server global Herdr agent delivery.
    HerdrAgent,
}

impl SubmissionRouteKind {
    /// Stable internal storage and diagnostics spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AdjacentPane => "adjacent_pane",
            Self::HerdrAgent => "herdr_agent",
        }
    }
}

/// One closed verified route for semantic prompt delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubmissionRoute {
    /// Existing same-tab directional delivery with independently verified geometry.
    AdjacentPane {
        /// Direction from the Proqi pane.
        direction: Direction,
        /// Current-server identity of the adjacent agent.
        target: HerdrAgentAddress,
        /// Source context against which adjacency was verified.
        source: PaneContext,
        /// Verified adjacent target geometry.
        target_rect: PaneRect,
    },
    /// Globally addressed coding agent on the current Herdr server.
    HerdrAgent(HerdrAgentAddress),
}

impl SubmissionRoute {
    /// Return the exact current-server target address.
    #[must_use]
    pub const fn target(&self) -> &HerdrAgentAddress {
        match self {
            Self::AdjacentPane { target, .. } | Self::HerdrAgent(target) => target,
        }
    }

    /// Return the verified adjacent direction, when this is an adjacent route.
    #[must_use]
    pub const fn adjacent_direction(&self) -> Option<Direction> {
        match self {
            Self::AdjacentPane { direction, .. } => Some(*direction),
            Self::HerdrAgent(_) => None,
        }
    }

    /// Return the adjacent source context, when geometry is part of this route.
    #[must_use]
    pub const fn adjacent_source(&self) -> Option<&PaneContext> {
        match self {
            Self::AdjacentPane { source, .. } => Some(source),
            Self::HerdrAgent(_) => None,
        }
    }

    /// Return the verified adjacent target rectangle, when present.
    #[must_use]
    pub const fn adjacent_target_rect(&self) -> Option<PaneRect> {
        match self {
            Self::AdjacentPane { target_rect, .. } => Some(*target_rect),
            Self::HerdrAgent(_) => None,
        }
    }

    /// Stable route-kind spelling used by persistence and diagnostics.
    #[must_use]
    pub const fn kind_str(&self) -> &'static str {
        self.kind().as_str()
    }

    /// Return the closed route classification.
    #[must_use]
    pub const fn kind(&self) -> SubmissionRouteKind {
        match self {
            Self::AdjacentPane { .. } => SubmissionRouteKind::AdjacentPane,
            Self::HerdrAgent(_) => SubmissionRouteKind::HerdrAgent,
        }
    }
}
