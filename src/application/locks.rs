//! Reducer-owned transient locks for in-flight submission sources.

use super::{Action, AppState, ApplicationError, ApplicationResult, Effect};
use crate::domain::{BoardMutation, ThoughtId, UndoScope};

pub(super) fn ensure_action_unlocked(state: &AppState, action: &Action) -> ApplicationResult<()> {
    let locked = match action {
        Action::EnterEdit(thought_id)
        | Action::EditThought { thought_id, .. }
        | Action::SplitThought { thought_id, .. }
        | Action::ExtractThought { thought_id, .. }
        | Action::DeleteThought { thought_id, .. }
        | Action::MoveThought { thought_id, .. }
        | Action::SetPresentation { thought_id, .. } => locked_one(state, *thought_id),
        Action::CutThoughts { thought_ids, .. }
        | Action::DeleteThoughts { thought_ids, .. }
        | Action::SetPresentationMany { thought_ids, .. }
        | Action::DuplicateThoughts { thought_ids, .. }
        | Action::MergeThoughts { thought_ids, .. } => locked_many(state, thought_ids),
        Action::Undo { scope, .. } => locked_history(state, *scope, true),
        Action::Redo { scope, .. } => locked_history(state, *scope, false),
        Action::RenameSession { .. }
        | Action::FocusThought(_)
        | Action::ExitEdit
        | Action::CreateThought { .. }
        | Action::PasteAsThought { .. }
        | Action::CopyThoughts { .. }
        | Action::ClipboardResult { .. }
        | Action::BeginSubmission { .. }
        | Action::EndSubmission { .. }
        | Action::PersistenceCommitted(_)
        | Action::PersistenceFailed { .. }
        | Action::RetryPersistence(_) => None,
    };
    locked.map_or(Ok(()), |thought_id| {
        Err(ApplicationError::ThoughtLocked(thought_id))
    })
}

pub(super) fn transition(state: &mut AppState, action: &Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::BeginSubmission { thought_ids } => {
            if thought_ids.is_empty() {
                return Err(ApplicationError::InvalidState);
            }
            for thought_id in thought_ids {
                state.live_thought(*thought_id)?;
                if state.locked_thoughts.contains(thought_id) {
                    return Err(ApplicationError::ThoughtLocked(*thought_id));
                }
            }
            state.locked_thoughts.extend(thought_ids);
        }
        Action::EndSubmission { thought_ids } => {
            for thought_id in thought_ids {
                state.locked_thoughts.remove(thought_id);
            }
        }
        _ => return Err(ApplicationError::InvalidState),
    }
    Ok(Vec::new())
}

fn locked_one(state: &AppState, thought_id: ThoughtId) -> Option<ThoughtId> {
    state.thought_locked(thought_id).then_some(thought_id)
}

fn locked_many(state: &AppState, thought_ids: &[ThoughtId]) -> Option<ThoughtId> {
    thought_ids
        .iter()
        .find(|thought_id| state.thought_locked(**thought_id))
        .copied()
}

fn locked_history(state: &AppState, scope: UndoScope, undo: bool) -> Option<ThoughtId> {
    match scope {
        UndoScope::Editor { thought_id } => locked_one(state, thought_id),
        UndoScope::Board => {
            let operation = if undo {
                state
                    .board_history_cursor
                    .checked_sub(1)
                    .and_then(|index| state.board_history.get(index))
            } else {
                state.board_history.get(state.board_history_cursor)
            }?;
            let mutation = if undo {
                &operation.inverse
            } else {
                &operation.forward
            };
            locked_mutation(state, mutation)
        }
    }
}

fn locked_mutation(state: &AppState, mutation: &BoardMutation) -> Option<ThoughtId> {
    match mutation {
        BoardMutation::Batch { mutations } => mutations
            .iter()
            .find_map(|mutation| locked_mutation(state, mutation)),
        BoardMutation::AddThought { thought } => locked_one(state, thought.id),
        BoardMutation::SetDeletion { thought_id, .. }
        | BoardMutation::MoveThought { thought_id, .. }
        | BoardMutation::ReplaceContent { thought_id, .. }
        | BoardMutation::SetPresentation { thought_id, .. }
        | BoardMutation::LegacySetCollapsed { thought_id, .. } => locked_one(state, *thought_id),
    }
}
