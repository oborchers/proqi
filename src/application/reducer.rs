//! Pure reducer and mutation helpers.

use super::{
    Action,
    error::{ApplicationError, ApplicationResult},
    locks,
};
use crate::application::model::{
    AppState, ClipboardIntent, DurabilityState, Effect, InteractionMode,
};

use super::mutations::bulk::{delete_thoughts, duplicate_thoughts, set_presentation_many};
use super::mutations::{
    create_thought, delete_thought, edit_thought, finish_clipboard, history_move, move_thought,
    request_clipboard, set_presentation,
};

/// Reduce one action into current state and ordered effects.
///
/// # Errors
///
/// Returns a typed error when an action violates current state or domain invariants.
pub fn reduce(state: &mut AppState, action: Action) -> ApplicationResult<Vec<Effect>> {
    locks::ensure_action_unlocked(state, &action)?;
    if matches!(state.durability, DurabilityState::Failed { .. }) && mutates_durable_state(&action)
    {
        return Err(ApplicationError::InvalidState);
    }
    match action {
        Action::RenameSession { name } => reduce_session_name(state, name),
        Action::FocusThought(_)
        | Action::EnterEdit(_)
        | Action::EnterCompose
        | Action::ExitCompose
        | Action::ExitEdit => reduce_navigation(state, &action),
        Action::CreateThought { .. }
        | Action::PasteAsThought { .. }
        | Action::EditThought { .. } => reduce_content(state, action),
        Action::CopyThoughts { .. }
        | Action::CutThoughts { .. }
        | Action::ClipboardResult { .. } => reduce_clipboard(state, &action),
        Action::BeginSubmission { .. } | Action::EndSubmission { .. } => {
            locks::transition(state, &action)
        }
        Action::DeleteThought { .. }
        | Action::DeleteThoughts { .. }
        | Action::MoveThought { .. }
        | Action::SetPresentation { .. }
        | Action::SetPresentationMany { .. }
        | Action::DuplicateThoughts { .. } => reduce_board(state, &action),
        Action::Undo { .. } | Action::Redo { .. } => reduce_history(state, &action),
        Action::PersistenceCommitted(_)
        | Action::PersistenceFailed { .. }
        | Action::RetryPersistence(_) => reduce_persistence(state, &action),
    }
}

const fn mutates_durable_state(action: &Action) -> bool {
    matches!(
        action,
        Action::CreateThought { .. }
            | Action::RenameSession { .. }
            | Action::PasteAsThought { .. }
            | Action::EditThought { .. }
            | Action::CutThoughts { .. }
            | Action::DeleteThought { .. }
            | Action::DeleteThoughts { .. }
            | Action::MoveThought { .. }
            | Action::SetPresentation { .. }
            | Action::SetPresentationMany { .. }
            | Action::DuplicateThoughts { .. }
            | Action::Undo { .. }
            | Action::Redo { .. }
    )
}

fn reduce_session_name(
    state: &mut AppState,
    name: Option<String>,
) -> ApplicationResult<Vec<Effect>> {
    let previous_name = state.board.session.name.clone();
    state.board.session.rename(name.clone())?;
    Ok(vec![Effect::RenameSession {
        session_id: state.board.session.id,
        previous_name,
        name,
    }])
}

fn reduce_navigation(state: &mut AppState, action: &Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::FocusThought(focus) => {
            if let Some(id) = *focus {
                state.live_thought(id)?;
            }
            state.focused_thought = *focus;
        }
        Action::EnterEdit(thought_id) => {
            state.live_thought(*thought_id)?;
            state.focused_thought = Some(*thought_id);
            state.mode = InteractionMode::Edit {
                thought_id: *thought_id,
            };
        }
        Action::EnterCompose => state.mode = InteractionMode::Compose,
        Action::ExitCompose | Action::ExitEdit => state.mode = InteractionMode::Board,
        _ => return Err(ApplicationError::InvalidState),
    }
    Ok(Vec::new())
}

fn reduce_content(state: &mut AppState, action: Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::CreateThought {
            thought_id,
            operation_id,
            content,
            annotations,
            insertion_index,
            at,
        } => create_thought(
            state,
            thought_id,
            operation_id,
            content,
            annotations,
            insertion_index.unwrap_or(state.insertion_index),
            at,
        ),
        Action::PasteAsThought {
            thought_id,
            operation_id,
            content,
            annotations,
            at,
        } => create_thought(
            state,
            thought_id,
            operation_id,
            content,
            annotations,
            state.insertion_index,
            at,
        ),
        Action::EditThought {
            thought_id,
            revision_id,
            before_content,
            after_content,
            before_annotations,
            after_annotations,
            before_cursor,
            after_cursor,
            at,
        } => edit_thought(
            state,
            thought_id,
            revision_id,
            before_content,
            after_content,
            before_annotations,
            after_annotations,
            before_cursor,
            after_cursor,
            at,
        ),
        _ => Err(ApplicationError::InvalidState),
    }
}

fn reduce_clipboard(state: &mut AppState, action: &Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::CopyThoughts {
            request_id,
            thought_ids,
        } => request_clipboard(
            state,
            *request_id,
            thought_ids,
            ClipboardIntent::Copy,
            None,
            None,
        ),
        Action::CutThoughts {
            request_id,
            operation_id,
            thought_ids,
            at,
        } => request_clipboard(
            state,
            *request_id,
            thought_ids,
            ClipboardIntent::Cut,
            Some(*operation_id),
            Some(*at),
        ),
        Action::ClipboardResult { request_id, result } => {
            let completion =
                if result.is_ok() && matches!(state.durability, DurabilityState::Failed { .. }) {
                    Err(crate::application::FailureCode::StorageFailed)
                } else {
                    *result
                };
            finish_clipboard(state, *request_id, completion)
        }
        _ => Err(ApplicationError::InvalidState),
    }
}

fn reduce_board(state: &mut AppState, action: &Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::DeleteThought {
            operation_id,
            thought_id,
            kind,
            at,
        } => delete_thought(state, *operation_id, *thought_id, *kind, *at),
        Action::DeleteThoughts {
            operation_id,
            thought_ids,
            kind,
            at,
        } => delete_thoughts(state, *operation_id, thought_ids, *kind, *at),
        Action::MoveThought {
            operation_id,
            thought_id,
            to,
            at,
        } => move_thought(state, *operation_id, *thought_id, *to, *at),
        Action::SetPresentation {
            operation_id,
            thought_id,
            presentation,
            at,
        } => set_presentation(state, *operation_id, *thought_id, *presentation, *at),
        Action::SetPresentationMany {
            operation_id,
            thought_ids,
            presentation,
            at,
        } => set_presentation_many(state, *operation_id, thought_ids, *presentation, *at),
        Action::DuplicateThoughts {
            operation_id,
            thought_ids,
            duplicate_ids,
            at,
        } => duplicate_thoughts(state, *operation_id, thought_ids, duplicate_ids, *at),
        _ => Err(ApplicationError::InvalidState),
    }
}

fn reduce_history(state: &mut AppState, action: &Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::Undo {
            operation_id,
            scope,
            at,
        } => history_move(state, *operation_id, *scope, *at, true),
        Action::Redo {
            operation_id,
            scope,
            at,
        } => history_move(state, *operation_id, *scope, *at, false),
        _ => Err(ApplicationError::InvalidState),
    }
}

fn reduce_persistence(state: &mut AppState, action: &Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::PersistenceCommitted(sequence) => {
            if state.pending_sequences.first().copied() != Some(*sequence) {
                return Err(ApplicationError::InvalidState);
            }
            state.pending_sequences.remove(sequence);
            state.board.session.last_durable_sequence =
                state.board.session.last_durable_sequence.max(*sequence);
            state.refresh_durability();
            Ok(Vec::new())
        }
        Action::PersistenceFailed { sequence, code } => {
            if !state.pending_sequences.contains(sequence) {
                return Err(ApplicationError::InvalidState);
            }
            let failed = match state.durability {
                DurabilityState::Failed { failed, .. } => failed.min(*sequence),
                DurabilityState::Durable { .. } | DurabilityState::Pending { .. } => *sequence,
            };
            state.durability = DurabilityState::Failed {
                durable: state.board.session.last_durable_sequence,
                failed,
                code: *code,
            };
            Ok(vec![Effect::Notify { code: *code }])
        }
        Action::RetryPersistence(sequence) => {
            if matches!(
                state.durability,
                DurabilityState::Failed {
                    code: crate::application::FailureCode::RecoveryCapacity,
                    ..
                }
            ) {
                return Err(ApplicationError::InvalidState);
            }
            if !matches!(
                state.durability,
                DurabilityState::Failed { failed, .. } if failed == *sequence
            ) || !state.pending_sequences.contains(sequence)
            {
                return Err(ApplicationError::InvalidState);
            }
            state.durability = DurabilityState::Pending {
                durable: state.board.session.last_durable_sequence,
                latest: *state
                    .pending_sequences
                    .last()
                    .ok_or(ApplicationError::InvalidState)?,
            };
            Ok(vec![Effect::RetryPersistence {
                sequence: *sequence,
            }])
        }
        _ => Err(ApplicationError::InvalidState),
    }
}
