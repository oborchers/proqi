//! Verified adjacent-agent discovery and semantic prompt submission.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

use crate::domain::{Direction, SubmissionId};

/// Terminal-cell rectangle reported by an integration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaneRect {
    /// Left edge.
    pub x: u16,
    /// Top edge.
    pub y: u16,
    /// Cell width.
    pub width: u16,
    /// Cell height.
    pub height: u16,
}

impl PaneRect {
    /// Exclusive right edge.
    #[must_use]
    pub const fn right(self) -> u16 {
        self.x.saturating_add(self.width)
    }

    /// Exclusive bottom edge.
    #[must_use]
    pub const fn bottom(self) -> u16 {
        self.y.saturating_add(self.height)
    }
}

/// Identity and geometry of the Proqi pane used for verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaneContext {
    /// Opaque integration workspace identifier.
    pub workspace_id: String,
    /// Opaque integration tab identifier.
    pub tab_id: String,
    /// Opaque integration pane identifier.
    pub pane_id: String,
    /// Current pane geometry.
    pub rect: PaneRect,
}

/// Installed integration capability negotiated with its live server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentCapabilities {
    /// Provider name.
    pub provider: String,
    /// Installed provider version.
    pub version: String,
    /// Negotiated protocol number.
    pub protocol: u32,
    /// Prompt delivery behaviors verified for this provider version.
    pub delivery: AgentDeliveryCapabilities,
    /// Current Proqi pane.
    pub context: PaneContext,
}

/// Durable board behavior after one accepted prompt submission.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionDisposition {
    /// Preserve the submitted thought.
    Keep,
    /// Delete the thought only after an accepted matching receipt.
    RemoveAfterSuccess,
}

impl SubmissionDisposition {
    /// Stable representation used by persistence and diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::RemoveAfterSuccess => "remove_after_success",
        }
    }
}

/// Negotiated delivery behaviors. Unsupported actions stay absent from the UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentDeliveryCapabilities {
    /// Immediate turn submission is available.
    pub submit: bool,
}

impl AgentDeliveryCapabilities {
    /// Current Herdr semantic contract: immediate submission only.
    pub const SUBMIT_ONLY: Self = Self { submit: true };

    /// Whether semantic immediate submission is explicitly supported.
    #[must_use]
    pub const fn supports(self) -> bool {
        self.submit
    }
}

/// Agent state known before submission.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Ready for new input.
    Idle,
    /// Currently processing and able to receive steering or queued input.
    Working,
    /// Settled after background work.
    Done,
    /// Harness reported an explicit blocked state.
    Blocked,
    /// Harness could not determine the current state.
    Unknown,
}

impl AgentState {
    /// Stable representation used by integrations and persistence.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Blocked => "blocked",
            Self::Unknown => "unknown",
        }
    }
}

/// Stable content-free classification for an agent integration failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentFailureCode {
    /// Integration is not available in the current environment.
    Unavailable,
    /// Integration cannot provide the requested semantic capability.
    Unsupported,
    /// Provider response violated the negotiated contract.
    Malformed,
    /// Provider returned more than one matching target.
    Ambiguous,
    /// A bounded provider operation timed out.
    TimedOut,
    /// Provider explicitly rejected the operation.
    Rejected,
    /// Provider process execution failed.
    ProcessFailed,
}

impl AgentFailureCode {
    /// Stable machine-readable representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Unsupported => "unsupported",
            Self::Malformed => "malformed",
            Self::Ambiguous => "ambiguous",
            Self::TimedOut => "timed_out",
            Self::Rejected => "rejected",
            Self::ProcessFailed => "process_failed",
        }
    }
}

/// One independently verified adjacent agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTarget {
    /// Integration provider that verified this target.
    pub provider: String,
    /// Negotiated provider protocol.
    pub protocol: u32,
    /// Direction from the Proqi pane.
    pub direction: Direction,
    /// Opaque target pane identifier.
    pub pane_id: String,
    /// Opaque workspace identifier matching the source.
    pub workspace_id: String,
    /// Opaque tab identifier matching the source.
    pub tab_id: String,
    /// Recognized harness kind.
    pub agent_kind: String,
    /// User-facing identity.
    pub agent_name: String,
    /// Stable harness session identity.
    pub agent_session_id: String,
    /// Verified readiness.
    pub readiness: AgentState,
    /// Delivery behaviors verified for this target.
    pub delivery: AgentDeliveryCapabilities,
    /// Target pane geometry.
    pub rect: PaneRect,
    /// Source context against which adjacency was verified.
    pub source: PaneContext,
}

/// Stable identity used to match discovery and submission receipts.
///
/// Geometry, readiness, display names, and negotiated delivery metadata may
/// legitimately change while one semantic prompt request is in flight.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTargetIdentity {
    /// Integration provider.
    pub provider: String,
    /// Integration workspace containing both panes.
    pub workspace_id: String,
    /// Integration tab containing both panes.
    pub tab_id: String,
    /// Source Proqi pane.
    pub source_pane_id: String,
    /// Target agent pane.
    pub target_pane_id: String,
    /// Verified direction from source to target.
    pub direction: Direction,
    /// Recognized agent harness.
    pub agent_kind: String,
    /// Stable harness session identity.
    pub agent_session_id: String,
}

impl AgentTarget {
    /// Return the stable receipt identity, excluding volatile presentation and state.
    #[must_use]
    pub fn identity(&self) -> AgentTargetIdentity {
        AgentTargetIdentity {
            provider: self.provider.clone(),
            workspace_id: self.workspace_id.clone(),
            tab_id: self.tab_id.clone(),
            source_pane_id: self.source.pane_id.clone(),
            target_pane_id: self.pane_id.clone(),
            direction: self.direction,
            agent_kind: self.agent_kind.clone(),
            agent_session_id: self.agent_session_id.clone(),
        }
    }
}

/// One semantic prompt submission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmissionRequest {
    /// Proqi-created receipt identity.
    pub submission_id: SubmissionId,
    /// Previously verified target, revalidated immediately before submission.
    pub target: AgentTarget,
    /// Exact thought content.
    pub content: String,
}

/// Confirmation that the harness accepted a prompt operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmissionReceipt {
    /// Proqi-created receipt identity.
    pub submission_id: SubmissionId,
    /// Revalidated target that accepted the prompt.
    pub target: AgentTarget,
    /// Advisory harness state returned after acceptance, when available.
    pub post_state: Option<AgentState>,
}

/// Fail-closed integration error. Every variant leaves the thought unchanged.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AgentError {
    /// Herdr is not installed or Proqi is not inside a managed pane.
    #[error("agent integration is unavailable: {0}")]
    Unavailable(String),
    /// Installed capability or protocol is unsupported.
    #[error("agent integration is unsupported: {0}")]
    Unsupported(String),
    /// Structured output did not match the installed contract.
    #[error("agent integration returned malformed output: {0}")]
    Malformed(String),
    /// More than one result claimed the same verified identity.
    #[error("agent target is ambiguous: {0}")]
    Ambiguous(String),
    /// A bounded process call timed out.
    #[error("agent integration timed out")]
    TimedOut,
    /// Herdr rejected the semantic prompt operation.
    #[error("agent submission was rejected ({code}): {message}")]
    Rejected {
        /// Stable provider code.
        code: String,
        /// Human-readable provider detail.
        message: String,
    },
    /// Child-process execution failed.
    #[error("agent integration process failed: {0}")]
    Process(String),
}

impl AgentError {
    /// Return the stable content-free failure classification.
    #[must_use]
    pub const fn stable_code(&self) -> AgentFailureCode {
        match self {
            Self::Unavailable(_) => AgentFailureCode::Unavailable,
            Self::Unsupported(_) => AgentFailureCode::Unsupported,
            Self::Malformed(_) => AgentFailureCode::Malformed,
            Self::Ambiguous(_) => AgentFailureCode::Ambiguous,
            Self::TimedOut => AgentFailureCode::TimedOut,
            Self::Rejected { .. } => AgentFailureCode::Rejected,
            Self::Process(_) => AgentFailureCode::ProcessFailed,
        }
    }
}

/// Optional provider-independent adjacent-agent boundary.
pub trait AgentGateway {
    /// Negotiate installed and live-server capabilities.
    ///
    /// # Errors
    ///
    /// Fails closed when the provider, protocol, or current pane is unavailable.
    fn capabilities(&mut self) -> Result<AgentCapabilities, AgentError>;

    /// Resolve and independently verify eligible targets in all four directions.
    ///
    /// # Errors
    ///
    /// Fails closed on stale context, ambiguous identity, or malformed topology.
    fn adjacent_targets(&mut self, context: &PaneContext) -> Result<Vec<AgentTarget>, AgentError>;

    /// Revalidate one target and submit exact text through a semantic provider command.
    ///
    /// # Errors
    ///
    /// Fails without modifying the thought when validation or submission is not accepted.
    fn submit(&mut self, request: SubmissionRequest) -> Result<SubmissionReceipt, AgentError>;
}

/// Optional display-only identity published to a terminal host.
pub trait PanePresentation {
    /// Publish or refresh Proqi's pane label without claiming an agent identity.
    ///
    /// # Errors
    ///
    /// Fails when display metadata is unsupported, rejected, or unavailable.
    fn publish(&mut self, pane_id: &str, sequence: u64, ttl: Duration) -> Result<(), AgentError>;

    /// Clear display metadata on a clean exit.
    ///
    /// # Errors
    ///
    /// Fails when the host cannot clear the owned display metadata.
    fn clear(&mut self, pane_id: &str, sequence: u64) -> Result<(), AgentError>;
}
