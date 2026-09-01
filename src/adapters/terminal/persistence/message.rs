//! Typed messages crossing the ordered persistence lane.

use crate::{
    application::ThoughtMutation,
    domain::{OperationSequence, RequestId, SessionId, SubmissionId, Timestamp},
    ports::{
        store::{
            CaptureCommit, CaptureCommitOutcome, CommitReceipt, OperationBatch, SessionHit,
            StoreError, StoredOperationRequest, SubmissionAttempt, SubmissionOutcome,
        },
        transfer::SessionTransferRequest,
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
        result: Result<(), StoreError>,
    },
    TransferSessions(Result<Vec<SessionHit>, StoreError>),
    ThoughtTransferred {
        request: SessionTransferRequest,
        result: Result<ThoughtMutation, String>,
    },
    Lookup {
        request_id: RequestId,
        result: Result<Option<StoredOperationRequest>, StoreError>,
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
    RenameSession {
        request_id: Option<RequestId>,
        session_id: SessionId,
        previous_name: Option<String>,
        name: Option<String>,
    },
    DiscoverTransferSessions {
        current_session_id: SessionId,
    },
    TransferThought(SessionTransferRequest),
    Retry(OperationSequence),
    Lookup {
        request_id: RequestId,
        identity: crate::ports::store::DurableIdentity,
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
