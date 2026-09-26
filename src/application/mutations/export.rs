//! One undoable Board operation that removes or replaces durably exported thoughts.

use super::{
    AppState, ApplicationError, ApplicationResult, BoardMutation, BoardOperationKind, Effect,
    Thought, ThoughtPosition,
    bulk::delete_thoughts,
    transform::{deletion, operation, restoration},
};
use crate::{
    application::{
        InteractionMode,
        capture::attachment_reference,
        export::{ExportBoardChange, ExportCompletion},
    },
    domain::BoardItemId,
};

pub(in crate::application) fn complete_export(
    state: &mut AppState,
    completion: &ExportCompletion,
) -> ApplicationResult<Vec<Effect>> {
    let sources = exact_sources(state, completion)?;
    match &completion.change {
        ExportBoardChange::Remove => delete_thoughts(
            state,
            completion.operation_id,
            &completion.thought_ids,
            BoardOperationKind::ExportAndRemove,
            completion.at,
        ),
        ExportBoardChange::ReplaceWithReference {
            reference_thought_id,
            path,
        } => {
            let first = sources.first().ok_or(ApplicationError::InvalidState)?;
            let position = first.position;
            let (content, annotation) = attachment_reference(path, false)?;
            let mut reference = Thought::new(
                *reference_thought_id,
                state.board.session.id,
                content,
                ThoughtPosition::new(position.get()),
                completion.at,
            );
            let mut annotations = vec![annotation];
            state.board.attachment_counters().assign(&mut annotations)?;
            reference.set_annotations(annotations)?;
            let mut forward = sources
                .iter()
                .rev()
                .map(|thought| deletion(thought, completion.at))
                .collect::<Vec<_>>();
            forward.push(BoardMutation::AddThought {
                thought: reference.clone(),
            });
            let mut inverse = vec![deletion(&reference, completion.at)];
            inverse.extend(
                sources
                    .iter()
                    .map(|thought| restoration(thought, completion.at)),
            );
            let operation = operation(
                state,
                completion.operation_id,
                BoardOperationKind::ExportAndReplace,
                forward,
                inverse,
                completion.at,
            )?;
            state.record_board_operation(&operation)?;
            state.focused_item = Some(BoardItemId::Thought(*reference_thought_id));
            state.mode = InteractionMode::Board;
            Ok(vec![Effect::CommitBoardOperation(operation)])
        }
    }
}

/// Require every exported source to be live, in Board order, and unchanged.
fn exact_sources(
    state: &AppState,
    completion: &ExportCompletion,
) -> ApplicationResult<Vec<Thought>> {
    if completion.thought_ids.is_empty()
        || completion.thought_ids.len() != completion.expected_sources.len()
    {
        return Err(ApplicationError::InvalidState);
    }
    let mut current = Vec::with_capacity(completion.thought_ids.len());
    for (thought_id, expected) in completion
        .thought_ids
        .iter()
        .zip(&completion.expected_sources)
    {
        if expected.id != *thought_id {
            return Err(ApplicationError::InvalidState);
        }
        let thought = state.live_thought(*thought_id)?;
        if thought.content != expected.content || thought.annotations != expected.annotations {
            return Err(ApplicationError::ContentConflict(*thought_id));
        }
        current.push(thought.clone());
    }
    let board_order = state
        .board
        .live_thoughts()
        .into_iter()
        .filter(|thought| completion.thought_ids.contains(&thought.id))
        .map(|thought| thought.id)
        .collect::<Vec<_>>();
    if board_order != completion.thought_ids {
        return Err(ApplicationError::InvalidState);
    }
    Ok(current)
}
