//! Internal state for asynchronous board operations.

use crate::{
    application::ClipboardIntent,
    domain::{OperationId, ThoughtId, Timestamp},
    ports::{
        agent::{AgentError, SubmissionDisposition, SubmissionReceipt, SubmissionRequest},
        editor::EditorSnapshot,
    },
};

pub(super) struct PendingEditorClipboard {
    pub(super) intent: ClipboardIntent,
    pub(super) before: EditorSnapshot,
}

pub(super) struct PendingSubmission {
    pub(super) request: SubmissionRequest,
    pub(super) sources: Vec<PendingSubmissionSource>,
    pub(super) at: Timestamp,
    pub(super) disposition: SubmissionDisposition,
    pub(super) deletion_operation_id: OperationId,
    pub(super) completion: Option<Result<SubmissionReceipt, AgentError>>,
}

pub(super) struct PendingSubmissionSource {
    pub(super) thought_id: ThoughtId,
    pub(super) source_digest: [u8; 32],
}

#[derive(Clone, Copy)]
pub(super) struct SubmissionMode {
    pub(super) disposition: SubmissionDisposition,
}
