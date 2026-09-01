//! Atomic screenshot capture persistence values.

use crate::{
    domain::{BoardOperation, OperationId, SessionId, ThoughtId, Timestamp},
    ports::screenshot::ScreenshotFingerprint,
};

use super::CommitReceipt;

/// One atomic screenshot receipt and prospective board operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureCommit {
    /// Rename-stable source identity.
    pub source: ScreenshotFingerprint,
    /// Exact append operation, applied only with the receipt.
    pub operation: BoardOperation,
}

/// Durable identity of one screenshot already delivered to a session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureReceipt {
    /// Rename-stable source identity.
    pub source: ScreenshotFingerprint,
    /// Session that received the screenshot.
    pub session_id: SessionId,
    /// Thought created for the screenshot.
    pub thought_id: ThoughtId,
    /// Structural operation that created the thought.
    pub operation_id: OperationId,
    /// Commit timestamp.
    pub accepted_at: Timestamp,
}

/// Atomic screenshot commit result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureCommitOutcome {
    /// The receipt and thought were created together.
    Created {
        /// Ordinary board durability receipt.
        durable: CommitReceipt,
        /// Durable capture receipt.
        capture: CaptureReceipt,
    },
    /// This source had already been delivered by an earlier owner or retry.
    AlreadyCaptured(CaptureReceipt),
}
