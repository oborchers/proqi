//! Typed messages crossing the ordered persistence lane.

use crate::{
    domain::{BrowserOperation, OperationSequence, RequestId, SessionId, SubmissionId, Timestamp},
    ports::{
        store::{
            BrowserCommitReceipt, CaptureCommit, CaptureCommitOutcome, CommitReceipt,
            OperationBatch, SessionHit, StoreError, StoredOperationRequest, SubmissionAttempt,
            SubmissionOutcome,
        },
        transfer::SessionTransferBatchRequest,
    },
};

pub(in crate::adapters::terminal) enum PersistenceResult {
    Capture(Result<CaptureCommitOutcome, StoreError>),
    Sequenced {
        sequence: OperationSequence,
        result: Result<CommitReceipt, StoreError>,
        retried: bool,
    },
    RetryFinished,
    Metadata {
        result: Result<(), StoreError>,
    },
    SessionRenamed {
        request_id: Option<RequestId>,
        previous_name: Option<String>,
        result: Result<BrowserCommitReceipt, StoreError>,
    },
    TransferSessions {
        generation: u64,
        result: Result<Vec<SessionHit>, StoreError>,
    },
    ThoughtsTransferred {
        request: SessionTransferBatchRequest,
        result: Result<CommitReceipt, String>,
    },
    TransferFinished {
        operation_id: crate::domain::OperationId,
        sequence: Option<OperationSequence>,
        result: Result<Option<CommitReceipt>, StoreError>,
        retried: bool,
    },
    Lookup {
        request_id: RequestId,
        result: Result<Option<StoredOperationRequest>, StoreError>,
    },
    BrowserNoOpRename {
        request_id: RequestId,
        result: Result<BrowserCommitReceipt, StoreError>,
    },
    SubmissionPrepared {
        submission_id: SubmissionId,
        result: Result<(), StoreError>,
    },
    SubmissionSending {
        submission_id: SubmissionId,
        result: Result<(), StoreError>,
    },
    SubmissionFinished {
        submission_id: SubmissionId,
        sequence: Option<OperationSequence>,
        result: Result<Option<CommitReceipt>, StoreError>,
        retried: bool,
    },
}

pub(super) enum PersistenceRequest {
    Capture(Box<CaptureCommit>),
    Commit(Box<OperationBatch>),
    Metadata(Box<OperationBatch>),
    BrowserOperation {
        request_id: Option<RequestId>,
        previous_name: Option<String>,
        operation: Box<BrowserOperation>,
    },
    DiscoverTransferSessions {
        current_session_id: SessionId,
        generation: u64,
    },
    TransferThoughts(SessionTransferBatchRequest),
    FinishTransfer {
        request: SessionTransferBatchRequest,
        removal: Option<Box<crate::domain::BoardOperation>>,
        reason: &'static str,
    },
    Retry(OperationSequence),
    Lookup {
        request_id: RequestId,
        identity: crate::ports::store::DurableIdentity,
    },
    BrowserNoOpRename {
        request_id: RequestId,
        operation_id: crate::domain::OperationId,
        session_id: SessionId,
        name: Option<String>,
        at: Timestamp,
    },
    PrepareSubmission(Box<SubmissionAttempt>),
    MarkSubmissionSending {
        submission_id: SubmissionId,
        at: Timestamp,
    },
    FinishSubmission {
        submission_id: SubmissionId,
        outcome: Box<SubmissionOutcome>,
        removal: Option<Box<crate::domain::BoardOperation>>,
    },
}
