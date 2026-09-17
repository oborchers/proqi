//! Routing for reversible Board operations.

use crate::application::{Action, AppState, ApplicationError, ApplicationResult, Effect};

use crate::application::mutations::{
    bulk::{
        delete_items, delete_thoughts, duplicate_items, duplicate_thoughts, set_presentation_many,
        stage_submission_removal,
    },
    delete_thought, move_item, move_thought, rename_thought, set_presentation,
    transform::merge_thoughts,
};

pub(super) fn reduce_board(
    state: &mut AppState,
    action: &Action,
) -> ApplicationResult<Vec<Effect>> {
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
        Action::DeleteItems {
            operation_id,
            item_ids,
            kind,
            at,
        } => delete_items(state, *operation_id, item_ids, *kind, *at),
        Action::StageSubmissionRemoval {
            operation_id,
            thought_ids,
            at,
        } => stage_submission_removal(state, *operation_id, thought_ids, *at),
        Action::MoveThought {
            operation_id,
            thought_id,
            to,
            at,
        } => move_thought(state, *operation_id, *thought_id, *to, *at),
        Action::MoveItem {
            operation_id,
            item_id,
            to,
            at,
        } => move_item(state, *operation_id, *item_id, *to, *at),
        Action::RenameThought {
            operation_id,
            thought_id,
            name,
            at,
        } => rename_thought(state, *operation_id, *thought_id, name.clone(), *at),
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
        Action::DuplicateItems {
            operation_id,
            item_ids,
            duplicate_ids,
            at,
        } => duplicate_items(state, *operation_id, item_ids, duplicate_ids, *at),
        Action::MergeThoughts {
            operation_id,
            thought_ids,
            expected_sources,
            separator,
            at,
        } => merge_thoughts(
            state,
            *operation_id,
            thought_ids,
            expected_sources,
            separator,
            *at,
        ),
        _ => Err(ApplicationError::InvalidState),
    }
}
