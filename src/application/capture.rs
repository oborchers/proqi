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

use super::{AppState, ApplicationError, ApplicationResult};

/// Build one prompt-ready image-path thought without exposing it before durable acceptance.
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
    let path = candidate
        .path
        .to_str()
        .ok_or(ApplicationError::InvalidState)?;
    let mut content = String::with_capacity(path.len().saturating_add(1));
    content.push_str(path);
    content.push(' ');
    let display_name = safe_display_name(&candidate.path)?;
    let annotation = ContentAnnotation {
        start: 0,
        end: path.len(),
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

/// Apply one already-durable capture without choosing terminal interaction state.
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
    state.apply_durable_capture(operation)?;
    state.insertion_index = state.board.live_thoughts().len();
    Ok(Some(thought_id))
}

fn safe_display_name(path: &Path) -> ApplicationResult<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .ok_or(ApplicationError::InvalidState)
}

#[cfg(test)]
mod tests {
    use crate::{
        adapters::memory::FakeIdGenerator,
        application::{AppState, attachments::attachment_keys},
        domain::{BoardMutation, Session, SessionBoard, Timestamp},
        ports::{
            environment::IdGenerator as _,
            screenshot::{ScreenshotCandidate, ScreenshotFingerprint, ScreenshotImageType},
        },
    };

    use super::prepare_capture;

    #[test]
    fn capture_content_has_one_suffix_while_annotation_and_health_keep_the_exact_path() {
        let mut ids = FakeIdGenerator::new(1_725_260_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir(),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let state = AppState::new(SessionBoard::new(session, Vec::new()).expect("board"));
        let candidate = ScreenshotCandidate {
            fingerprint: ScreenshotFingerprint([7; 32]),
            path: std::env::temp_dir().join("Unicode capture with spaces 🖼️.png"),
            image_type: ScreenshotImageType::Png,
        };
        let commit = prepare_capture(
            &state,
            &candidate,
            ids.thought_id(),
            ids.operation_id(),
            Timestamp::from_millis(2),
        )
        .expect("capture");
        let BoardMutation::AddThought { thought } = &commit.operation.forward else {
            panic!("capture thought");
        };
        let path = candidate.path.to_str().expect("UTF-8 path");
        assert_eq!(thought.content, format!("{path} "));
        assert_eq!(thought.annotations[0].end, path.len());
        let keys = attachment_keys(thought);
        assert_eq!(keys[0].canonical_path, path);
        assert_eq!(keys[0].annotation_end, path.len());
    }
}
