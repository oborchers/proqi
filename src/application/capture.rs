//! Commit-first screenshot thought construction.

use std::path::Path;

use crate::{
    domain::{
        BoardMutation, BoardOperation, BoardOperationKind, ContentAnnotation,
        ContentAnnotationKind, OperationId, Thought, ThoughtId, ThoughtPosition, Timestamp,
    },
    ports::{
        screenshot::ScreenshotCandidate,
        store::{CaptureCommit, CaptureCommitOutcome},
    },
};

use super::{AppState, ApplicationError, ApplicationResult, InteractionMode};

/// Build one exact image-path thought without exposing it before durable acceptance.
///
/// # Errors
///
/// Returns an invalid-state error when the path cannot satisfy the exact annotation contract or
/// the next durable operation cannot be constructed.
pub fn prepare_capture(
    state: &AppState,
    candidate: &ScreenshotCandidate,
    thought_id: ThoughtId,
    operation_id: OperationId,
    at: Timestamp,
) -> ApplicationResult<CaptureCommit> {
    let content = candidate
        .path
        .to_str()
        .ok_or(ApplicationError::InvalidState)?
        .to_owned();
    let display_name = safe_display_name(&candidate.path)?;
    let annotation = ContentAnnotation {
        start: 0,
        end: content.len(),
        kind: ContentAnnotationKind::Attachment {
            image: true,
            display_name,
        },
    };
    let sequence = state.next_sequence()?;
    let insertion_index = state.board.live_thoughts().len();
    let position = u32::try_from(insertion_index)
        .map(ThoughtPosition::new)
        .map_err(|_| ApplicationError::InvalidState)?;
    let mut thought = Thought::new(thought_id, state.board.session.id, content, position, at);
    thought.set_annotations(vec![annotation])?;
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence,
        kind: BoardOperationKind::Create,
        forward: BoardMutation::AddThought {
            thought: thought.clone(),
        },
        inverse: BoardMutation::SetDeletion {
            thought_id,
            deleted_at: Some(at),
            position,
        },
        created_at: at,
    };
    Ok(CaptureCommit {
        source: candidate.fingerprint,
        operation,
    })
}

/// Apply one already-durable capture without disturbing an active editor.
///
/// # Errors
///
/// Returns an invalid-state error when the durable receipt does not match the proposed capture or
/// the operation cannot be applied to the current board.
pub fn apply_capture(
    state: &mut AppState,
    commit: &CaptureCommit,
    outcome: &CaptureCommitOutcome,
) -> ApplicationResult<Option<ThoughtId>> {
    let CaptureCommitOutcome::Created { durable, capture } = outcome else {
        return Ok(None);
    };
    if durable.sequence != commit.operation.sequence
        || durable.identity != crate::ports::store::DurableIdentity::Operation(commit.operation.id)
        || capture.source != commit.source
    {
        return Err(ApplicationError::InvalidState);
    }
    let operation = &commit.operation;
    let thought_id = match &operation.forward {
        BoardMutation::AddThought { thought } => thought.id,
        _ => return Err(ApplicationError::InvalidState),
    };
    let preserve_edit = matches!(state.mode, InteractionMode::Edit { .. });
    state.apply_durable_capture(operation)?;
    state.insertion_index = state.board.live_thoughts().len();
    if !preserve_edit {
        state.focused_thought = Some(thought_id);
        state.mode = InteractionMode::Edit { thought_id };
    }
    Ok(Some(thought_id))
}

fn safe_display_name(path: &Path) -> ApplicationResult<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .ok_or(ApplicationError::InvalidState)
}
