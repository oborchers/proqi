//! Blocking work requested by the pure reducer.

use crate::{
    domain::{
        BoardOperation, ContentAnnotation, OperationId, OperationSequence, RequestId, SessionId,
        ThoughtId, ThoughtRevision, Timestamp, UndoScope,
    },
    ports::{
        agent::{AgentTarget, SubmissionRequest},
        attachment_accessibility::AttachmentCheckBatch,
        invocation::{InvocationDiscoveryRequest, InvocationReferenceDiscoveryRequest},
        recovery::RecoveryDocument,
        store::{OperationBatch, SubmissionAttempt, SubmissionOutcome},
        transfer::SessionTransferBatchRequest,
    },
};

use super::{ClipboardIntent, FailureCode, ScreenshotIntent, ScreenshotPauseReason, UpdateIntent};

/// One external or durable effect emitted by the reducer and application UI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Verify exact transient attachment revisions on the bounded accessibility lane.
    CheckAttachments(AttachmentCheckBatch),
    /// Start, stop, or explicitly take over screenshot capture.
    Screenshot(ScreenshotIntent),
    /// Present one best-effort terminal-host notification after truthful automatic pause.
    NotifyScreenshotPause(ScreenshotPauseReason),
    /// Atomically commit one screenshot receipt and its exact thought.
    CommitCapture(crate::ports::store::CaptureCommit),
    /// Execute one explicit installation-wide update decision outside the reducer lane.
    Update(UpdateIntent),
    /// Discover live destination sessions for an explicit transfer picker.
    DiscoverTransferSessions {
        /// Picker generation used to discard a completion from an earlier owner.
        generation: u64,
    },
    /// Deliver one thought cohort as one durable destination operation.
    TransferThoughts(SessionTransferBatchRequest),
    /// Complete a selected transfer journal, atomically committing source removal when present.
    FinishTransfer {
        /// Exact prepared cohort.
        request: SessionTransferBatchRequest,
        /// Exact source removal, if the accepted cohort can be removed.
        removal: Option<BoardOperation>,
        /// Stable completion classification.
        reason: &'static str,
    },
    /// Persist one installation-wide Browser administration operation.
    CommitBrowserOperation(crate::domain::BrowserOperation),
    /// Discover verified adjacent agents without blocking the reducer lane.
    DiscoverAgents,
    /// Discover compatible coding agents across the current Herdr server.
    DiscoverGlobalAgents {
        /// Picker generation used to discard stale completion.
        generation: u64,
    },
    /// Refresh bounded authoring definitions without blocking the reducer lane.
    DiscoverInvocations(InvocationDiscoveryRequest),
    /// Refresh one bounded picker-open collaborator snapshot.
    DiscoverInvocationReferences(InvocationReferenceDiscoveryRequest),
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
        /// Accepted source removal committed atomically with the terminal journal row.
        removal: Option<BoardOperation>,
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
    /// Reserve one same-value thought rename without adding a history unit.
    CommitThoughtNoOpRename {
        /// Durable operation identity.
        operation_id: OperationId,
        /// Owning session.
        session_id: SessionId,
        /// Affected thought.
        thought_id: ThoughtId,
        /// Current and requested name.
        name: Option<crate::domain::ThoughtName>,
        /// Monotonic durable sequence.
        sequence: OperationSequence,
        /// Operation time.
        at: Timestamp,
    },
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
        /// Source thought for thought or editor content; absent for session metadata.
        thought_id: Option<ThoughtId>,
        /// Typed copy or cut behavior.
        intent: ClipboardIntent,
        /// Exact clipboard content.
        content: String,
        /// Validated selection-relative presentation metadata.
        annotations: Vec<ContentAnnotation>,
    },
    /// Read exact content from the native clipboard.
    ReadClipboard {
        /// External request identity.
        request_id: RequestId,
    },
    /// Atomically write one plain-text thought export, durable before it reports success.
    WriteExport {
        /// External request identity.
        request_id: RequestId,
        /// Exact destination, bytes, and replacement policy.
        request: crate::ports::export::ExportWriteRequest,
    },
    /// List one directory for export destination completion.
    ListExportDirectory {
        /// Completion generation used to discard a stale listing.
        generation: u64,
        /// Absolute directory to list.
        directory: std::path::PathBuf,
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
            Self::CommitBoardOperation(operation) => Some(OperationBatch::Board {
                operation: operation.clone(),
                semantic_fingerprint: None,
            }),
            Self::CommitThoughtNoOpRename {
                operation_id,
                session_id,
                thought_id,
                name,
                sequence,
                at,
            } => Some(OperationBatch::ThoughtNoOpRename {
                operation_id: *operation_id,
                session_id: *session_id,
                thought_id: *thought_id,
                name: name.clone(),
                sequence: *sequence,
                at: *at,
                semantic_fingerprint: None,
            }),
            Self::CommitRevision(revision) => Some(OperationBatch::Revision {
                revision: revision.clone(),
                semantic_fingerprint: None,
            }),
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
                semantic_fingerprint: None,
            }),
            _ => None,
        }
    }
}

/// One sequenced durable mutation and the auxiliary work emitted with it.
#[derive(Debug)]
pub(crate) struct SequencedMutationEffects {
    /// Canonical durable store request.
    pub(crate) batch: OperationBatch,
    /// Sequence owned by the durable request.
    pub(crate) sequence: OperationSequence,
    /// Auxiliary effects that must follow the durable enqueue.
    pub(crate) auxiliary: Vec<Effect>,
}

impl SequencedMutationEffects {
    /// Split one reducer mutation without accepting unrelated effect kinds.
    pub(crate) fn new(effects: Vec<Effect>) -> Result<Self, SequencedMutationEffectError> {
        let mut batch = None;
        let mut auxiliary = Vec::new();
        for effect in effects {
            if let Some(candidate) = effect.persistence_batch() {
                assign_durable_batch(&mut batch, candidate)?;
            } else if matches!(effect, Effect::CheckAttachments(_)) {
                auxiliary.push(effect);
            } else {
                return Err(SequencedMutationEffectError::UnsupportedAuxiliary);
            }
        }
        let batch = batch.ok_or(SequencedMutationEffectError::MissingDurable)?;
        let sequence = batch
            .sequence()
            .ok_or(SequencedMutationEffectError::MissingSequence)?;
        Ok(Self {
            batch,
            sequence,
            auxiliary,
        })
    }
}

fn assign_durable_batch(
    batch: &mut Option<OperationBatch>,
    candidate: OperationBatch,
) -> Result<(), SequencedMutationEffectError> {
    if batch.replace(candidate).is_some() {
        return Err(SequencedMutationEffectError::MultipleDurable);
    }
    Ok(())
}

/// Invalid effect shape for an ordinary sequenced reducer mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SequencedMutationEffectError {
    /// No durable mutation was emitted.
    MissingDurable,
    /// The durable mutation did not own a session sequence.
    MissingSequence,
    /// More than one durable mutation was emitted.
    MultipleDurable,
    /// An effect other than attachment reconciliation accompanied the mutation.
    UnsupportedAuxiliary,
    /// The durable batch could not be paired with the exact public request.
    InvalidSemanticFingerprint,
}
