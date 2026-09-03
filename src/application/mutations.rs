//! Focused domain mutations used by the reducer router.

pub(super) mod bulk;
pub(super) mod transform;

use super::error::{ApplicationError, ApplicationResult, FailureCode};
use super::{mutations::bulk::delete_thoughts, prompt::MULTI_THOUGHT_SEPARATOR};
use crate::{
    application::model::{
        AppState, ClipboardIntent, Effect, InteractionMode,
        clipboard::{ClipboardSource, PendingClipboard},
    },
    domain::{
        BoardMutation, BoardOperation, BoardOperationKind, ContentAnnotation, DomainError,
        OperationId, RequestId, RevisionId, TextPosition, Thought, ThoughtId, ThoughtPosition,
        ThoughtPresentation, ThoughtRevision, Timestamp, UndoScope, merge_annotations,
        validate_annotations,
    },
};

pub(super) fn create_thought(
    state: &mut AppState,
    thought_id: ThoughtId,
    operation_id: OperationId,
    content: String,
    annotations: Vec<ContentAnnotation>,
    insertion_index: usize,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let sequence = state.next_sequence()?;
    validate_annotations(&content, &annotations)?;
    let mut thought = Thought::new(
        thought_id,
        state.board.session.id,
        content,
        ThoughtPosition::new(position_u32(insertion_index)?),
        at,
    );
    thought.set_annotations(annotations)?;
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
            position: thought.position,
        },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    state.focused_thought = Some(thought_id);
    state.mode = InteractionMode::Edit { thought_id };
    state.insertion_index = insertion_index + 1;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

#[expect(
    clippy::too_many_arguments,
    reason = "an exact revision keeps before and after content and cursor state together"
)]
pub(super) fn edit_thought(
    state: &mut AppState,
    thought_id: ThoughtId,
    revision_id: RevisionId,
    before_content: String,
    after_content: String,
    before_annotations: Vec<ContentAnnotation>,
    after_annotations: Vec<ContentAnnotation>,
    before_cursor: TextPosition,
    after_cursor: TextPosition,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let current = state.live_thought(thought_id)?;
    if current.content != before_content || current.annotations != before_annotations {
        return Err(ApplicationError::RevisionConflict(thought_id));
    }
    validate_annotations(&after_content, &after_annotations)?;
    if before_content == after_content && before_annotations == after_annotations {
        return Ok(Vec::new());
    }
    let sequence = state.next_sequence()?;
    let revision = ThoughtRevision {
        id: revision_id,
        session_id: state.board.session.id,
        thought_id,
        sequence,
        before_content,
        after_content: after_content.clone(),
        before_annotations,
        after_annotations: after_annotations.clone(),
        before_cursor,
        after_cursor,
        created_at: at,
    };
    let mut board = state.board.clone();
    let thought = board
        .thought_mut(thought_id)
        .ok_or(ApplicationError::ThoughtNotFound(thought_id))?;
    thought.content = after_content;
    thought.annotations = after_annotations;
    thought.updated_at = at;
    state.board = board;
    state.truncate_conflicting_board_redo(thought_id);
    let history = state.editor_histories.entry(thought_id).or_default();
    history.revisions.truncate(history.cursor);
    history.revisions.push(revision.clone());
    history.cursor += 1;
    state.track_pending(sequence);
    Ok(vec![Effect::CommitRevision(revision)])
}

pub(super) fn request_clipboard(
    state: &mut AppState,
    request_id: RequestId,
    thought_ids: &[ThoughtId],
    intent: ClipboardIntent,
    operation_id: Option<OperationId>,
    at: Option<Timestamp>,
) -> ApplicationResult<Vec<Effect>> {
    if thought_ids.is_empty() {
        return Err(ApplicationError::InvalidState);
    }
    let selected = state
        .board
        .live_thoughts()
        .into_iter()
        .filter(|thought| thought_ids.contains(&thought.id))
        .collect::<Vec<_>>();
    if selected.len() != thought_ids.len() {
        return Err(ApplicationError::InvalidState);
    }
    let sources = selected
        .iter()
        .map(|thought| ClipboardSource::capture(thought))
        .collect::<Vec<_>>();
    let content = selected
        .iter()
        .map(|thought| thought.content.as_str())
        .collect::<Vec<_>>()
        .join(MULTI_THOUGHT_SEPARATOR);
    let annotations = merge_annotations(
        selected
            .iter()
            .map(|thought| (thought.content.as_str(), thought.annotations.as_slice())),
        MULTI_THOUGHT_SEPARATOR,
    )?;
    let thought_id = sources[0].thought_id;
    state
        .pending_clipboard
        .entry(request_id)
        .or_insert(PendingClipboard {
            sources,
            intent,
            operation_id,
            at: at.unwrap_or_default(),
        });
    Ok(vec![Effect::WriteClipboard {
        request_id,
        thought_id: Some(thought_id),
        intent,
        content,
        annotations,
    }])
}

pub(super) fn finish_clipboard(
    state: &mut AppState,
    request_id: RequestId,
    result: Result<(), FailureCode>,
) -> ApplicationResult<Vec<Effect>> {
    let Some(pending) = state.pending_clipboard.remove(&request_id) else {
        return Ok(Vec::new());
    };
    if let Err(code) = result {
        return Ok(vec![Effect::Notify { code }]);
    }
    if pending.intent == ClipboardIntent::Cut {
        let source_changed = pending.sources.iter().any(|source| {
            state
                .board
                .thought(source.thought_id)
                .is_none_or(|thought| !source.still_matches(thought))
        });
        if source_changed {
            return Ok(vec![Effect::Notify {
                code: FailureCode::ContentConflict,
            }]);
        }
        let thought_ids = pending
            .sources
            .iter()
            .map(|source| source.thought_id)
            .collect::<Vec<_>>();
        if let Some(thought_id) = pending
            .sources
            .iter()
            .map(|source| source.thought_id)
            .find(|thought_id| state.thought_locked(*thought_id))
        {
            return Err(ApplicationError::ThoughtLocked(thought_id));
        }
        return delete_thoughts(
            state,
            pending.operation_id.ok_or(ApplicationError::InvalidState)?,
            &thought_ids,
            BoardOperationKind::Cut,
            pending.at,
        );
    }
    Ok(Vec::new())
}
pub(super) fn delete_thought(
    state: &mut AppState,
    operation_id: OperationId,
    thought_id: ThoughtId,
    kind: BoardOperationKind,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let deleted_index = state
        .board
        .live_thoughts()
        .iter()
        .position(|candidate| candidate.id == thought_id)
        .ok_or(ApplicationError::InvalidState)?;
    let was_focused = state.focused_thought == Some(thought_id);
    let operation = build_delete_thought_operation(state, operation_id, thought_id, kind, at)?;
    state.record_board_operation(&operation)?;
    if was_focused {
        let live = state.board.live_thoughts();
        state.focused_thought = live
            .get(deleted_index)
            .or_else(|| {
                deleted_index
                    .checked_sub(1)
                    .and_then(|previous| live.get(previous))
            })
            .map(|thought| thought.id);
    }
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(super) fn build_delete_thought_operation(
    state: &AppState,
    operation_id: OperationId,
    thought_id: ThoughtId,
    kind: BoardOperationKind,
    at: Timestamp,
) -> ApplicationResult<BoardOperation> {
    if !matches!(
        kind,
        BoardOperationKind::Delete | BoardOperationKind::Cut | BoardOperationKind::SubmitAndRemove
    ) {
        return Err(ApplicationError::InvalidState);
    }
    let thought = state.live_thought(thought_id)?.clone();
    let sequence = state.next_sequence()?;
    Ok(BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence,
        kind,
        forward: BoardMutation::SetDeletion {
            thought_id,
            deleted_at: Some(at),
            position: thought.position,
        },
        inverse: BoardMutation::SetDeletion {
            thought_id,
            deleted_at: None,
            position: thought.position,
        },
        created_at: at,
    })
}

pub(super) fn move_thought(
    state: &mut AppState,
    operation_id: OperationId,
    thought_id: ThoughtId,
    to: usize,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let thought = state.live_thought(thought_id)?;
    let from = thought.position;
    let to = ThoughtPosition::new(position_u32(to)?);
    if from == to {
        return Ok(Vec::new());
    }
    let sequence = state.next_sequence()?;
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence,
        kind: BoardOperationKind::Reorder,
        forward: BoardMutation::MoveThought {
            thought_id,
            from,
            to,
        },
        inverse: BoardMutation::MoveThought {
            thought_id,
            from: to,
            to: from,
        },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(super) fn set_presentation(
    state: &mut AppState,
    operation_id: OperationId,
    thought_id: ThoughtId,
    presentation: ThoughtPresentation,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let previous = state.live_thought(thought_id)?.presentation;
    if previous == presentation {
        return Ok(Vec::new());
    }
    let sequence = state.next_sequence()?;
    let operation = BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence,
        kind: BoardOperationKind::Collapse,
        forward: BoardMutation::SetPresentation {
            thought_id,
            presentation,
        },
        inverse: BoardMutation::SetPresentation {
            thought_id,
            presentation: previous,
        },
        created_at: at,
    };
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(super) fn history_move(
    state: &mut AppState,
    operation_id: OperationId,
    scope: UndoScope,
    at: Timestamp,
    undo: bool,
) -> ApplicationResult<Vec<Effect>> {
    let sequence = state.next_sequence()?;
    match scope {
        UndoScope::Board => move_board_history(state, at, undo)?,
        UndoScope::Editor { thought_id } => move_editor_history(state, thought_id, at, undo)?,
    }
    state.track_pending(sequence);
    Ok(vec![Effect::CommitHistoryMove {
        operation_id,
        session_id: state.board.session.id,
        scope,
        undo,
        sequence,
        at,
    }])
}

fn move_board_history(state: &mut AppState, at: Timestamp, undo: bool) -> ApplicationResult<()> {
    let operation = if undo {
        state
            .board_history_cursor
            .checked_sub(1)
            .and_then(|index| state.board_history.get(index))
    } else {
        state.board_history.get(state.board_history_cursor)
    }
    .cloned()
    .ok_or(ApplicationError::InvalidState)?;
    let mutation = if undo {
        &operation.inverse
    } else {
        &operation.forward
    };
    let focused_before = state.focused_thought;
    let transform_source = undo.then(|| transform_source(&operation)).flatten();
    let mut board = state.board.clone();
    board.apply_mutation(mutation, at)?;
    state.board = board;
    state.board_history_cursor =
        state
            .board_history_cursor
            .saturating_add_signed(if undo { -1 } else { 1 });
    let focus_was_removed = focused_before.is_some_and(|thought_id| {
        state
            .board
            .thought(thought_id)
            .is_none_or(|thought| !thought.is_live())
    });
    state.keep_focus_valid();
    if focus_was_removed
        && let Some(thought_id) = transform_source
        && state
            .board
            .thought(thought_id)
            .is_some_and(Thought::is_live)
    {
        state.focused_thought = Some(thought_id);
    }
    Ok(())
}

fn transform_source(operation: &BoardOperation) -> Option<ThoughtId> {
    if !matches!(
        operation.kind,
        BoardOperationKind::Split | BoardOperationKind::Extract
    ) {
        return None;
    }
    replaced_thought(&operation.forward)
}

fn replaced_thought(mutation: &BoardMutation) -> Option<ThoughtId> {
    match mutation {
        BoardMutation::Batch { mutations } => mutations.iter().find_map(replaced_thought),
        BoardMutation::ReplaceContent { thought_id, .. } => Some(*thought_id),
        BoardMutation::AddThought { .. }
        | BoardMutation::SetDeletion { .. }
        | BoardMutation::SetDeletionExact { .. }
        | BoardMutation::MoveThought { .. }
        | BoardMutation::SetPresentation { .. }
        | BoardMutation::LegacySetCollapsed { .. } => None,
    }
}

fn move_editor_history(
    state: &mut AppState,
    thought_id: ThoughtId,
    at: Timestamp,
    undo: bool,
) -> ApplicationResult<()> {
    let history = state
        .editor_histories
        .get(&thought_id)
        .ok_or(ApplicationError::InvalidState)?;
    let revision = if undo {
        history
            .cursor
            .checked_sub(1)
            .and_then(|index| history.revisions.get(index))
    } else {
        history.revisions.get(history.cursor)
    }
    .cloned()
    .ok_or(ApplicationError::InvalidState)?;
    let (expected, expected_annotations, content, annotations) = if undo {
        (
            &revision.after_content,
            &revision.after_annotations,
            revision.before_content,
            revision.before_annotations,
        )
    } else {
        (
            &revision.before_content,
            &revision.before_annotations,
            revision.after_content,
            revision.after_annotations,
        )
    };
    let current = state.live_thought(thought_id)?;
    if &current.content != expected || &current.annotations != expected_annotations {
        return Err(ApplicationError::RevisionConflict(thought_id));
    }
    let thought = state
        .board
        .thought_mut(thought_id)
        .ok_or(ApplicationError::ThoughtNotFound(thought_id))?;
    thought.content = content;
    thought.annotations = annotations;
    thought.updated_at = at;
    let history = state
        .editor_histories
        .get_mut(&thought_id)
        .ok_or(ApplicationError::InvalidState)?;
    history.cursor = history
        .cursor
        .saturating_add_signed(if undo { -1 } else { 1 });
    Ok(())
}

fn position_u32(value: usize) -> Result<u32, ApplicationError> {
    u32::try_from(value).map_err(|_| {
        ApplicationError::Domain(DomainError::InvalidPosition {
            requested: value,
            len: usize::try_from(u32::MAX).unwrap_or(usize::MAX),
        })
    })
}
