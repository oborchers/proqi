//! Typed messages crossing the ordered persistence lane.

use crate::{
    application::ThoughtMutation,
    domain::{OperationId, OperationSequence, RequestId, SessionId, SubmissionId, Timestamp},
    ports::{
        store::{
            CommitReceipt, OperationBatch, SessionHit, StoreError, StoredOperationRequest,
            SubmissionAttempt, SubmissionOutcome,
        },
        transfer::SessionTransferRequest,
    },
};

pub(in crate::adapters::terminal) enum PersistenceResult {
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
        result: Result<(), StoreError>,
    },
}

pub(super) enum PersistenceRequest {
    Commit(Box<OperationBatch>),
    Metadata(Box<OperationBatch>),
    RenameSession {
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
        operation_id: OperationId,
    },
    PrepareSubmission(Box<SubmissionAttempt>),
    MarkSubmissionSending {
        submission_id: SubmissionId,
        at: Timestamp,
    },
    FinishSubmission {
        submission_id: SubmissionId,
        outcome: Box<SubmissionOutcome>,
    },
}
