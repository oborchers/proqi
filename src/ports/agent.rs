//! Verified adjacent-agent discovery and semantic prompt submission.

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
    /// Current Proqi pane.
    pub context: PaneContext,
}

/// Agent state known before submission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentReadiness {
    /// Ready for new input.
    Idle,
    /// Currently processing and able to receive steering or queued input.
    Working,
    /// Settled after background work.
    Done,
}

/// One independently verified adjacent agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTarget {
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
    pub readiness: AgentReadiness,
    /// Target pane geometry.
    pub rect: PaneRect,
    /// Source context against which adjacency was verified.
    pub source: PaneContext,
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
    /// Harness state returned after acceptance.
    pub readiness: AgentReadiness,
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
