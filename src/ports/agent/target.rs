//! Verified delivery target and receipt identity.

use crate::domain::Direction;

use super::{
    AgentDeliveryCapabilities, AgentSessionBinding, AgentState, HarnessKind, HerdrAgentAddress,
    PaneContext, PaneRect, SubmissionRoute, SubmissionRouteKind,
};

/// Truthful live eligibility for semantic prompt delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentAvailability {
    /// The provider supports prompt delivery in the observed live state.
    Available,
    /// The harness explicitly reports that it is blocked.
    Blocked,
    /// The harness state cannot currently be determined.
    Unknown,
    /// The harness is still launching.
    Launching,
    /// The pane is not ready for interactive input.
    NotInteractive,
}

impl AgentAvailability {
    /// Stable content-redacted state spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Blocked => "blocked",
            Self::Unknown => "unknown",
            Self::Launching => "launching",
            Self::NotInteractive => "not_interactive",
        }
    }
}

/// One independently verified adjacent or current-server global agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTarget {
    /// Integration provider that verified this target.
    pub provider: String,
    /// Negotiated provider protocol.
    pub protocol: u32,
    /// Closed verified adjacent or current-server global route.
    pub route: SubmissionRoute,
    /// User-facing identity.
    pub agent_name: String,
    /// Optional bounded workspace label from the verified discovery snapshot.
    pub workspace_label: Option<String>,
    /// Optional bounded tab label from the verified discovery snapshot.
    pub tab_label: Option<String>,
    /// Verified readiness.
    pub readiness: AgentState,
    /// Current live eligibility, including launch and interactive readiness.
    pub availability: AgentAvailability,
    /// Delivery behaviors verified for this target.
    pub delivery: AgentDeliveryCapabilities,
}

/// Stable identity used to match discovery and submission receipts.
///
/// Geometry, readiness, display names, and negotiated delivery metadata may
/// legitimately change while one semantic prompt request is in flight.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTargetIdentity {
    /// Integration provider.
    pub provider: String,
    /// Closed delivery route classification.
    pub route_kind: SubmissionRouteKind,
    /// Integration workspace containing the target.
    pub workspace_id: String,
    /// Integration tab containing the target.
    pub tab_id: String,
    /// Source Proqi pane for adjacent delivery only.
    pub source_pane_id: Option<String>,
    /// Target agent pane.
    pub target_pane_id: String,
    /// Verified direction from source to target for adjacent delivery only.
    pub direction: Option<Direction>,
    /// Recognized agent harness.
    pub agent_kind: HarnessKind,
    /// Stable or provisional harness session binding.
    pub agent_session: AgentSessionBinding,
}

impl AgentTarget {
    /// Construct one verified adjacent target.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "closed adjacent route has nine independent facts"
    )]
    pub fn adjacent(
        provider: String,
        protocol: u32,
        direction: Direction,
        address: HerdrAgentAddress,
        agent_name: String,
        readiness: AgentState,
        delivery: AgentDeliveryCapabilities,
        target_rect: PaneRect,
        source: PaneContext,
    ) -> Self {
        Self {
            provider,
            protocol,
            route: SubmissionRoute::AdjacentPane {
                direction,
                target: address,
                source,
                target_rect,
            },
            agent_name,
            workspace_label: None,
            tab_label: None,
            readiness,
            availability: AgentAvailability::Available,
            delivery,
        }
    }

    /// Construct one current-server global target.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "closed global route has eight independent facts"
    )]
    pub fn herdr_agent(
        protocol: u32,
        address: HerdrAgentAddress,
        agent_name: String,
        workspace_label: Option<String>,
        tab_label: Option<String>,
        readiness: AgentState,
        availability: AgentAvailability,
        delivery: AgentDeliveryCapabilities,
    ) -> Self {
        Self {
            provider: "herdr".to_owned(),
            protocol,
            route: SubmissionRoute::HerdrAgent(address),
            agent_name,
            workspace_label,
            tab_label,
            readiness,
            availability,
            delivery,
        }
    }

    /// Return the exact verified current-server target address.
    #[must_use]
    pub const fn address(&self) -> &HerdrAgentAddress {
        self.route.target()
    }

    /// Return the target pane identity.
    #[must_use]
    pub fn pane_id(&self) -> &str {
        self.address().pane_id()
    }

    /// Return the target workspace identity.
    #[must_use]
    pub fn workspace_id(&self) -> &str {
        self.address().workspace_id()
    }

    /// Return the target tab identity.
    #[must_use]
    pub fn tab_id(&self) -> &str {
        self.address().tab_id()
    }

    /// Return the recognized harness kind.
    #[must_use]
    pub const fn agent_kind(&self) -> &HarnessKind {
        self.address().agent_kind()
    }

    /// Return the stable or explicitly provisional session binding.
    #[must_use]
    pub const fn agent_session(&self) -> &AgentSessionBinding {
        self.address().agent_session()
    }

    /// Return the adjacent direction, when this is an adjacent route.
    #[must_use]
    pub const fn adjacent_direction(&self) -> Option<Direction> {
        self.route.adjacent_direction()
    }

    /// Whether this exact observed target is eligible for semantic delivery.
    #[must_use]
    pub const fn can_submit(&self) -> bool {
        self.delivery.supports() && matches!(self.availability, AgentAvailability::Available)
    }

    /// Return this target with a replacement recognized harness identity.
    #[must_use]
    pub fn with_agent_kind(mut self, kind: HarnessKind) -> Self {
        self.address_mut().replace_agent_kind(kind);
        self
    }

    /// Return this target with a replacement stable or qualified provisional session.
    #[must_use]
    pub fn with_agent_session(mut self, session: AgentSessionBinding) -> Self {
        self.bind_agent_session(session);
        self
    }

    /// Replace volatile adjacent geometry while preserving the delivery identity.
    #[must_use]
    pub fn with_adjacent_geometry(mut self, target_rect: PaneRect, source: PaneContext) -> Self {
        if let SubmissionRoute::AdjacentPane {
            target_rect: current_rect,
            source: current_source,
            ..
        } = &mut self.route
        {
            *current_rect = target_rect;
            *current_source = source;
        }
        self
    }

    /// Replace a provisional session with the exact accepted receipt binding.
    pub(crate) fn bind_agent_session(&mut self, binding: AgentSessionBinding) {
        self.address_mut().replace_agent_session(binding);
    }

    #[cfg(test)]
    pub(crate) fn set_test_agent_kind(&mut self, kind: HarnessKind) {
        self.address_mut().replace_agent_kind(kind);
    }

    #[cfg(test)]
    pub(crate) fn set_test_agent_session(&mut self, session: AgentSessionBinding) {
        self.address_mut().replace_agent_session(session);
    }

    /// Return the stable receipt identity, excluding volatile presentation and state.
    #[must_use]
    pub fn identity(&self) -> AgentTargetIdentity {
        AgentTargetIdentity {
            provider: self.provider.clone(),
            route_kind: self.route.kind(),
            workspace_id: self.workspace_id().to_owned(),
            tab_id: self.tab_id().to_owned(),
            source_pane_id: self
                .route
                .adjacent_source()
                .map(|source| source.pane_id.clone()),
            target_pane_id: self.pane_id().to_owned(),
            direction: self.adjacent_direction(),
            agent_kind: self.agent_kind().clone(),
            agent_session: self.agent_session().clone(),
        }
    }

    /// Whether a receipt preserves an established or provisional target identity.
    #[must_use]
    pub fn accepts_receipt(&self, receipt: &Self) -> bool {
        let expected = self.identity();
        let actual = receipt.identity();
        expected.provider == actual.provider
            && expected.route_kind == actual.route_kind
            && expected.workspace_id == actual.workspace_id
            && expected.tab_id == actual.tab_id
            && expected.source_pane_id == actual.source_pane_id
            && expected.target_pane_id == actual.target_pane_id
            && expected.direction == actual.direction
            && expected.agent_kind == actual.agent_kind
            && expected
                .agent_session
                .accepts_receipt(&actual.agent_session)
    }

    fn address_mut(&mut self) -> &mut HerdrAgentAddress {
        match &mut self.route {
            SubmissionRoute::AdjacentPane { target, .. } | SubmissionRoute::HerdrAgent(target) => {
                target
            }
        }
    }
}
