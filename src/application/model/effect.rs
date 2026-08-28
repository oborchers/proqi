//! Blocking work requested by the pure reducer.

use crate::{
    domain::{
        BoardOperation, OperationId, OperationSequence, RequestId, SessionId, ThoughtId,
        ThoughtRevision, Timestamp, UndoScope,
    },
    ports::{
        agent::{AgentTarget, SubmissionRequest},
        invocation::InvocationDiscoveryRequest,
        recovery::RecoveryDocument,
        store::{OperationBatch, SubmissionAttempt, SubmissionOutcome},
        transfer::SessionTransferRequest,
    },
};

use super::{ClipboardIntent, FailureCode, UpdateIntent};

/// One external or durable effect emitted by the reducer and application UI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Execute one explicit installation-wide update decision outside the reducer lane.
    Update(UpdateIntent),
    /// Discover live destination sessions for an explicit transfer picker.
    DiscoverTransferSessions,
    /// Copy one exact thought to another session before optional source removal.
    TransferThought(SessionTransferRequest),
    /// Persist an optimistic current-session rename.
    RenameSession {
        /// Owning session.
        session_id: SessionId,
        /// Previous name restored after failure.
        previous_name: Option<String>,
        /// Replacement name, or none to clear it.
        name: Option<String>,
    },
    /// Discover verified adjacent agents without blocking the reducer lane.
    DiscoverAgents,
    /// Refresh bounded authoring definitions without blocking the reducer lane.
    DiscoverInvocations(InvocationDiscoveryRequest),
    /// Submit exact thought content through a verified semantic agent gateway.
    SubmitAgent(SubmissionRequest),
    /// Durably prepare a redacted submission attempt before external delivery.
    PrepareSubmission(SubmissionAttempt),
    /// Mark a prepared attempt as externally in flight.
    MarkSubmissionSending {
        /// Submission identity.
        submission_id: crate::domain::SubmissionId,
        /// Transition time.
        at: Timestamp,
    },
    /// Durably record the final external outcome.
    FinishSubmission {
        /// Submission identity.
        submission_id: crate::domain::SubmissionId,
        /// Content-redacted terminal outcome.
        outcome: SubmissionOutcome,
    },
    /// Persist recognition-only context after an accepted submission.
    StoreIntegrationContext {
        /// Owning session.
        session_id: SessionId,
        /// Verified adjacent target.
        target: AgentTarget,
        /// Target verification time.
        verified_at: Timestamp,
    },
    /// Commit one new structural operation.
    CommitBoardOperation(BoardOperation),
    /// Commit one new editor revision.
    CommitRevision(ThoughtRevision),
    /// Atomically move one persistent history cursor and current state.
    CommitHistoryMove {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Owning session.
        session_id: SessionId,
        /// Board or thought history.
        scope: UndoScope,
        /// True for undo, false for redo.
        undo: bool,
        /// Monotonic durable sequence.
        sequence: OperationSequence,
        /// Operation time.
        at: Timestamp,
    },
    /// Write exact content through the clipboard adapter.
    WriteClipboard {
        /// External request identity.
        request_id: RequestId,
        /// Source thought.
        thought_id: ThoughtId,
        /// Copy or cut behavior.
        intent: ClipboardIntent,
        /// Exact clipboard content.
        content: String,
    },
    /// Read exact content from the native clipboard.
    ReadClipboard {
        /// External request identity.
        request_id: RequestId,
    },
    /// Atomically export the current in-memory board for recovery.
    ExportRecovery {
        /// External request identity.
        request_id: RequestId,
        /// Exact recovery document.
        document: Box<RecoveryDocument>,
    },
    /// Present a non-destructive user-visible status.
    Notify {
        /// Stable status classification.
        code: FailureCode,
    },
    /// Retry the retained durable batch for one sequence.
    RetryPersistence {
        /// First retained sequence to retry.
        sequence: OperationSequence,
    },
}

impl Effect {
    /// Convert a persistence effect into its durable store request.
    #[must_use]
    pub fn persistence_batch(&self) -> Option<OperationBatch> {
        match self {
            Self::CommitBoardOperation(operation) => Some(OperationBatch::Board(operation.clone())),
            Self::CommitRevision(revision) => Some(OperationBatch::Revision(revision.clone())),
            Self::CommitHistoryMove {
                operation_id,
                session_id,
                scope,
                undo,
                sequence,
                at,
            } => Some(OperationBatch::HistoryMove {
                operation_id: *operation_id,
                session_id: *session_id,
                scope: *scope,
                undo: *undo,
                sequence: *sequence,
                at: *at,
            }),
            _ => None,
        }
    }
}
