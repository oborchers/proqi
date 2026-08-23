//! Pure reducer and mutation helpers.

use crate::application::model::{
    Action, AppState, ApplicationError, ApplicationResult, ClipboardIntent, DurabilityState,
    Effect, InteractionMode,
};

use super::mutations::{
    create_thought, delete_thought, edit_thought, finish_clipboard, history_move, move_thought,
    request_clipboard, set_collapsed,
};

/// Reduce one action into current state and ordered effects.
///
/// # Errors
///
/// Returns a typed error when an action violates current state or domain invariants.
pub fn reduce(state: &mut AppState, action: Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::FocusThought(_) | Action::EnterEdit(_) | Action::ExitEdit => {
            reduce_navigation(state, &action)
        }
        Action::CreateThought { .. }
        | Action::PasteAsThought { .. }
        | Action::EditThought { .. } => reduce_content(state, action),
        Action::CopyThought { .. } | Action::CutThought { .. } | Action::ClipboardResult { .. } => {
            reduce_clipboard(state, &action)
        }
        Action::DeleteThought { .. } | Action::MoveThought { .. } | Action::SetCollapsed { .. } => {
            reduce_board(state, &action)
        }
        Action::Undo { .. } | Action::Redo { .. } => reduce_history(state, &action),
        Action::PersistenceCommitted(_) | Action::PersistenceFailed { .. } => {
            reduce_persistence(state, &action)
        }
    }
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
        Action::ExitEdit => state.mode = InteractionMode::Board,
        _ => unreachable!("navigation reducer received another action"),
    }
    Ok(Vec::new())
}

fn reduce_content(state: &mut AppState, action: Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::CreateThought {
            thought_id,
            operation_id,
            content,
            insertion_index,
            at,
        } => create_thought(
            state,
            thought_id,
            operation_id,
            content,
            insertion_index.unwrap_or(state.insertion_index),
            at,
        ),
        Action::PasteAsThought {
            thought_id,
            operation_id,
            content,
            at,
        } => create_thought(
            state,
            thought_id,
            operation_id,
            content,
            state.insertion_index,
            at,
        ),
        Action::EditThought {
            thought_id,
            revision_id,
            before_content,
            after_content,
            before_cursor,
            after_cursor,
            at,
        } => edit_thought(
            state,
            thought_id,
            revision_id,
            before_content,
            after_content,
            before_cursor,
            after_cursor,
            at,
        ),
        _ => unreachable!("content reducer received another action"),
    }
}

fn reduce_clipboard(state: &mut AppState, action: &Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::CopyThought {
            request_id,
            thought_id,
        } => request_clipboard(
            state,
            *request_id,
            *thought_id,
            ClipboardIntent::Copy,
            None,
            None,
        ),
        Action::CutThought {
            request_id,
            operation_id,
            thought_id,
            at,
        } => request_clipboard(
            state,
            *request_id,
            *thought_id,
            ClipboardIntent::Cut,
            Some(*operation_id),
            Some(*at),
        ),
        Action::ClipboardResult { request_id, result } => {
            finish_clipboard(state, *request_id, *result)
        }
        _ => unreachable!("clipboard reducer received another action"),
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
        Action::MoveThought {
            operation_id,
            thought_id,
            to,
            at,
        } => move_thought(state, *operation_id, *thought_id, *to, *at),
        Action::SetCollapsed {
            operation_id,
            thought_id,
            collapsed,
            at,
        } => set_collapsed(state, *operation_id, *thought_id, *collapsed, *at),
        _ => unreachable!("board reducer received another action"),
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
        _ => unreachable!("history reducer received another action"),
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
            state.durability = DurabilityState::Failed {
                durable: state.board.session.last_durable_sequence,
                failed: *sequence,
                code: *code,
            };
            Ok(vec![Effect::Notify { code: *code }])
        }
        _ => unreachable!("persistence reducer received another action"),
    }
}
