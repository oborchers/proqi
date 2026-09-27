//! Pure reducer and mutation helpers.

mod board;

use super::{
    Action, OwnedThoughtCreation, OwnedThoughtEdit,
    error::{ApplicationError, ApplicationResult},
    locks,
};
use crate::application::model::{
    AppState, ClipboardIntent, DurabilityState, Effect, InteractionMode,
};

use super::mutations::transform::{ExactSource, extract_thought, split_thought};
use super::mutations::{
    create_compose_thought, create_thought, edit_thought, finish_clipboard, history_move,
    insert_separator, rename_session, request_clipboard,
};
use board::reduce_board;

/// Reduce one action into current state and ordered effects.
///
/// # Errors
///
/// Returns a typed error when an action violates current state or domain invariants.
pub fn reduce(state: &mut AppState, action: Action) -> ApplicationResult<Vec<Effect>> {
    locks::ensure_action_unlocked(state, &action)?;
    if matches!(state.durability, DurabilityState::Failed { .. }) && action.mutates_durable_state()
    {
        return Err(ApplicationError::InvalidState);
    }
    let previous_focus = state.focused_item;
    let mut effects = match action {
        Action::RenameSession {
            operation_id,
            name,
            at,
        } => rename_session(state, operation_id, name, at),
        Action::FocusThought(_)
        | Action::FocusItem(_)
        | Action::EnterEdit(_)
        | Action::EnterCompose
        | Action::ExitCompose
        | Action::ExitEdit => reduce_navigation(state, &action),
        Action::CreateThought { .. }
        | Action::InsertSeparator { .. }
        | Action::CreateComposeThought { .. }
        | Action::CreateOwnedThought(_)
        | Action::CreateOwnedThoughts { .. }
        | Action::PasteAsThought { .. }
        | Action::EditThought { .. }
        | Action::EditOwnedThought(_) => reduce_content(state, action),
        Action::ReflowThought(reflow) => super::mutations::transform::reflow_thought(state, reflow),
        Action::ReflowThoughts(batch) => super::mutations::transform::reflow_thoughts(state, batch),
        Action::SplitThought { .. } | Action::ExtractThought { .. } => {
            reduce_content_transform(state, action)
        }
        Action::CopyThoughts { .. }
        | Action::CutThoughts { .. }
        | Action::ClipboardResult { .. } => reduce_clipboard(state, &action),
        Action::BeginSubmission { .. } | Action::EndSubmission { .. } => {
            locks::transition(state, &action)
        }
        Action::DeleteThought { .. }
        | Action::DeleteThoughts { .. }
        | Action::DeleteItems { .. }
        | Action::StageSubmissionRemoval { .. }
        | Action::MoveThought { .. }
        | Action::RenameThought { .. }
        | Action::MoveItem { .. }
        | Action::MoveItems { .. }
        | Action::SetPresentation { .. }
        | Action::SetPresentationMany { .. }
        | Action::DuplicateThoughts { .. }
        | Action::DuplicateItems { .. }
        | Action::MergeThoughts { .. }
        | Action::CompleteExport(_) => reduce_board(state, &action),
        Action::Undo { .. } | Action::Redo { .. } => reduce_history(state, &action),
        Action::PersistenceCommitted(_)
        | Action::PersistenceFailed { .. }
        | Action::RetryPersistence(_) => reduce_persistence(state, &action),
    }?;
    effects.extend(state.attachments.reconcile(&state.board));
    if state.focused_item != previous_focus
        && let Some(thought_id) = state.focused_thought_id()
    {
        effects.extend(state.attachments.prioritize_focus(thought_id));
    }
    Ok(effects)
}

fn reduce_navigation(state: &mut AppState, action: &Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::FocusThought(focus) => {
            if let Some(id) = *focus {
                state.live_thought(id)?;
            }
            state.focused_item = focus.map(crate::domain::BoardItemId::Thought);
        }
        Action::FocusItem(focus) => {
            if let Some(id) = *focus
                && state.board.item_position(id).is_none()
            {
                return Err(ApplicationError::InvalidState);
            }
            state.focused_item = *focus;
        }
        Action::EnterEdit(thought_id) => {
            state.live_thought(*thought_id)?;
            state.focused_item = Some(crate::domain::BoardItemId::Thought(*thought_id));
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
        } => {
            reject_new_shortcut_annotations(&after_annotations)?;
            edit_thought(
                state,
                thought_id,
                revision_id,
                before_content,
                after_content,
                before_annotations,
                after_annotations,
                before_cursor,
                None,
                after_cursor,
                None,
                at,
            )
        }
        Action::EditOwnedThought(edit) => edit_owned_thought(state, edit),
        action => reduce_creation(state, action),
    }
}

fn reduce_creation(state: &mut AppState, action: Action) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::CreateThought {
            thought_id,
            operation_id,
            content,
            annotations,
            insertion_index,
            at,
        } => {
            reject_new_shortcut_annotations(&annotations)?;
            create_thought(
                state,
                thought_id,
                operation_id,
                content,
                annotations,
                insertion_index.unwrap_or(state.insertion_index),
                at,
            )
        }
        Action::InsertSeparator {
            separator_id,
            operation_id,
            insertion_index,
            at,
        } => insert_separator(state, separator_id, operation_id, insertion_index, at),
        Action::CreateComposeThought {
            thought_id,
            operation_id,
            content,
            annotations,
            cursor,
            selection_anchor,
            preserve_owned,
            at,
        } => create_compose_thought(
            state,
            thought_id,
            operation_id,
            content,
            annotations,
            cursor,
            selection_anchor,
            preserve_owned,
            at,
        ),
        Action::CreateOwnedThought(creation) => create_owned_thought(state, creation),
        Action::CreateOwnedThoughts {
            operation_id,
            items,
            at,
        } => super::mutations::bulk::create_owned_thoughts(state, operation_id, &items, at),
        Action::PasteAsThought {
            thought_id,
            operation_id,
            content,
            annotations,
            at,
        } => {
            reject_new_shortcut_annotations(&annotations)?;
            create_thought(
                state,
                thought_id,
                operation_id,
                content,
                annotations,
                state.insertion_index,
                at,
            )
        }
        _ => Err(ApplicationError::InvalidState),
    }
}

fn reduce_content_transform(
    state: &mut AppState,
    action: Action,
) -> ApplicationResult<Vec<Effect>> {
    match action {
        Action::SplitThought {
            thought_id,
            new_thought_id,
            operation_id,
            expected_content,
            expected_annotations,
            source_content,
            source_annotations,
            at_byte,
            at,
        } => split_thought(
            state,
            operation_id,
            new_thought_id,
            &ExactSource {
                thought_id,
                expected_content,
                expected_annotations,
                content: source_content,
                annotations: source_annotations,
            },
            at_byte,
            at,
        ),
        Action::ExtractThought {
            thought_id,
            new_thought_id,
            operation_id,
            expected_content,
            expected_annotations,
            source_content,
            source_annotations,
            range,
            at,
        } => extract_thought(
            state,
            operation_id,
            new_thought_id,
            &ExactSource {
                thought_id,
                expected_content,
                expected_annotations,
                content: source_content,
                annotations: source_annotations,
            },
            range,
            at,
        ),
        _ => Err(ApplicationError::InvalidState),
    }
}

fn create_owned_thought(
    state: &mut AppState,
    creation: OwnedThoughtCreation,
) -> ApplicationResult<Vec<Effect>> {
    super::mutations::create_thought_with_handoff(
        state,
        creation.thought_id,
        creation.operation_id,
        creation.content,
        creation.annotations,
        creation.insertion_index.unwrap_or(state.insertion_index),
        None,
        false,
        creation.name,
        creation.at,
    )
}

fn edit_owned_thought(
    state: &mut AppState,
    edit: OwnedThoughtEdit,
) -> ApplicationResult<Vec<Effect>> {
    edit_thought(
        state,
        edit.thought_id,
        edit.revision_id,
        edit.before_content,
        edit.after_content,
        edit.before_annotations,
        edit.after_annotations,
        edit.before_cursor,
        edit.before_selection_anchor,
        edit.after_cursor,
        edit.after_selection_anchor,
        edit.at,
    )
}

fn reject_new_shortcut_annotations(
    annotations: &[crate::domain::ContentAnnotation],
) -> ApplicationResult<()> {
    if annotations
        .iter()
        .any(crate::domain::ContentAnnotation::is_shortcut_emphasis)
    {
        Err(ApplicationError::InvalidState)
    } else {
        Ok(())
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
            state.commit_deferred_board_operation(*sequence)?;
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
