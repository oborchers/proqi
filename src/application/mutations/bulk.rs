//! Atomic board mutations over an explicit thought selection.

use super::{
    AppState, ApplicationError, ApplicationResult, BoardItemId, BoardMutation, BoardOperation,
    BoardOperationKind, Effect, OperationId, Thought, ThoughtId, ThoughtPosition,
    ThoughtPresentation, Timestamp,
};
use crate::domain::Separator;
use crate::ports::transfer::TransferItem;

pub(in crate::application) fn create_owned_thoughts(
    state: &mut AppState,
    operation_id: OperationId,
    items: &[TransferItem],
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if items.is_empty()
        || items.iter().enumerate().any(|(index, item)| {
            item.source_thought_id == item.destination_thought_id
                || items[..index].iter().any(|previous| {
                    previous.source_thought_id == item.source_thought_id
                        || previous.destination_thought_id == item.destination_thought_id
                })
        })
    {
        return Err(ApplicationError::InvalidState);
    }
    let mut counters = state.board.attachment_counters();
    let start = state.board.live_items().len();
    let mut additions = Vec::new();
    let mut removals = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut annotations = item.annotations.clone();
        crate::domain::validate_annotations(&item.content, &annotations)?;
        crate::domain::renew_attachment_occurrences(&mut annotations);
        counters.assign(&mut annotations)?;
        let position = ThoughtPosition::new(super::position_u32(start + index)?);
        let mut thought = Thought::new(
            item.destination_thought_id,
            state.board.session.id,
            item.content.clone(),
            position,
            at,
        );
        thought.set_annotations(annotations)?;
        thought.set_name(item.name.clone());
        additions.push(BoardMutation::AddThought { thought });
        removals.push(BoardMutation::SetDeletion {
            thought_id: item.destination_thought_id,
            deleted_at: Some(at),
            position,
        });
    }
    removals.reverse();
    let operation = batch_operation(
        state,
        operation_id,
        BoardOperationKind::Create,
        additions,
        removals,
        at,
    )?;
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(in crate::application) fn delete_thoughts(
    state: &mut AppState,
    operation_id: OperationId,
    thought_ids: &[ThoughtId],
    kind: BoardOperationKind,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if thought_ids.len() == 1 {
        return super::delete_thought(state, operation_id, thought_ids[0], kind, at);
    }
    validate_deletion(thought_ids, kind)?;
    let selected = selected_thoughts(state, thought_ids)?;
    let first_index = usize::try_from(selected[0].position.get()).unwrap_or(usize::MAX);
    let focus_removed = state.focused_item.is_some_and(|focus| {
        thought_ids
            .iter()
            .any(|id| focus == BoardItemId::Thought(*id))
    });
    let forward = selected
        .iter()
        .rev()
        .map(|thought| deletion(thought, Some(at)))
        .collect();
    let inverse = selected
        .iter()
        .map(|thought| deletion(thought, None))
        .collect();
    let operation = batch_operation(state, operation_id, kind, forward, inverse, at)?;
    state.record_board_operation(&operation)?;
    if focus_removed {
        let live = state.board.live_items();
        state.focused_item = live
            .get(first_index)
            .or_else(|| first_index.checked_sub(1).and_then(|index| live.get(index)))
            .map(|item| item.id());
    }
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(in crate::application) fn stage_submission_removal(
    state: &mut AppState,
    operation_id: OperationId,
    thought_ids: &[ThoughtId],
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if thought_ids
        .iter()
        .any(|thought_id| !state.thought_locked(*thought_id))
    {
        return Err(ApplicationError::InvalidState);
    }
    let operation = if thought_ids.len() == 1 {
        super::build_delete_thought_operation(
            state,
            operation_id,
            thought_ids[0],
            BoardOperationKind::SubmitAndRemove,
            at,
        )?
    } else {
        validate_deletion(thought_ids, BoardOperationKind::SubmitAndRemove)?;
        let selected = selected_thoughts(state, thought_ids)?;
        let forward = selected
            .iter()
            .rev()
            .map(|thought| deletion(thought, Some(at)))
            .collect();
        let inverse = selected
            .iter()
            .map(|thought| deletion(thought, None))
            .collect();
        batch_operation(
            state,
            operation_id,
            BoardOperationKind::SubmitAndRemove,
            forward,
            inverse,
            at,
        )?
    };
    state.stage_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(in crate::application) fn set_presentation_many(
    state: &mut AppState,
    operation_id: OperationId,
    thought_ids: &[ThoughtId],
    presentation: ThoughtPresentation,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if thought_ids.len() == 1 {
        return super::set_presentation(state, operation_id, thought_ids[0], presentation, at);
    }
    let changed = selected_thoughts(state, thought_ids)?
        .into_iter()
        .filter(|thought| thought.presentation != presentation)
        .map(|thought| (thought.id, thought.presentation))
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return Ok(Vec::new());
    }
    let forward = changed
        .iter()
        .map(|(id, _)| presentation_mutation(*id, presentation))
        .collect();
    let inverse = changed
        .iter()
        .map(|(id, previous)| presentation_mutation(*id, *previous))
        .collect();
    let operation = batch_operation(
        state,
        operation_id,
        BoardOperationKind::Collapse,
        forward,
        inverse,
        at,
    )?;
    state.record_board_operation(&operation)?;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(in crate::application) fn duplicate_thoughts(
    state: &mut AppState,
    operation_id: OperationId,
    thought_ids: &[ThoughtId],
    duplicate_ids: &[ThoughtId],
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if thought_ids.len() != duplicate_ids.len() || thought_ids.is_empty() {
        return Err(ApplicationError::InvalidState);
    }
    let selected = selected_thoughts(state, thought_ids)?;
    let insertion = usize::try_from(
        selected
            .last()
            .ok_or(ApplicationError::InvalidState)?
            .position
            .get(),
    )
    .map_err(|_| ApplicationError::InvalidState)?
    .saturating_add(1);
    let mut counters = state.board.attachment_counters();
    let duplicates = selected
        .iter()
        .zip(duplicate_ids)
        .enumerate()
        .map(|(offset, (source, duplicate_id))| {
            let mut duplicate = source.clone();
            crate::domain::renew_attachment_occurrences(&mut duplicate.annotations);
            counters.assign(&mut duplicate.annotations)?;
            duplicate.id = *duplicate_id;
            duplicate.position = ThoughtPosition::new(super::position_u32(insertion + offset)?);
            duplicate.created_at = at;
            duplicate.updated_at = at;
            duplicate.deleted_at = None;
            Ok(duplicate)
        })
        .collect::<ApplicationResult<Vec<_>>>()?;
    let forward = duplicates
        .iter()
        .cloned()
        .map(|thought| BoardMutation::AddThought { thought })
        .collect();
    let inverse = duplicates
        .iter()
        .rev()
        .map(|thought| deletion(thought, Some(at)))
        .collect();
    let operation = batch_operation(
        state,
        operation_id,
        BoardOperationKind::Duplicate,
        forward,
        inverse,
        at,
    )?;
    state.record_board_operation(&operation)?;
    state.focused_item = duplicate_ids.first().copied().map(BoardItemId::Thought);
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(in crate::application) fn duplicate_items(
    state: &mut AppState,
    operation_id: OperationId,
    item_ids: &[BoardItemId],
    duplicate_ids: &[BoardItemId],
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if item_ids.len() != duplicate_ids.len() || item_ids.is_empty() {
        return Err(ApplicationError::InvalidState);
    }
    let selected = state
        .board
        .live_items()
        .into_iter()
        .filter(|item| item_ids.contains(&item.id()))
        .collect::<Vec<_>>();
    if !matches_exact_item_order(&selected, item_ids) {
        return Err(ApplicationError::InvalidState);
    }
    let insertion = selected
        .last()
        .and_then(|item| usize::try_from(item.position().get()).ok())
        .ok_or(ApplicationError::InvalidState)?
        .saturating_add(1);
    let mut counters = state.board.attachment_counters();
    let mut forward = Vec::with_capacity(selected.len());
    let mut inverse = Vec::with_capacity(selected.len());
    for (offset, (source, duplicate_id)) in selected.iter().zip(duplicate_ids).enumerate() {
        let position = ThoughtPosition::new(super::position_u32(insertion + offset)?);
        match (source, duplicate_id) {
            (crate::domain::BoardItemRef::Thought(source), BoardItemId::Thought(duplicate_id)) => {
                let mut duplicate = (*source).clone();
                crate::domain::renew_attachment_occurrences(&mut duplicate.annotations);
                counters.assign(&mut duplicate.annotations)?;
                duplicate.id = *duplicate_id;
                duplicate.position = position;
                duplicate.created_at = at;
                duplicate.updated_at = at;
                duplicate.deleted_at = None;
                forward.push(BoardMutation::AddThought {
                    thought: duplicate.clone(),
                });
                inverse.push(deletion(&duplicate, Some(at)));
            }
            (
                crate::domain::BoardItemRef::Separator(source),
                BoardItemId::Separator(duplicate_id),
            ) => {
                let duplicate = Separator::new(*duplicate_id, source.session_id, position, at);
                forward.push(BoardMutation::AddSeparator {
                    separator: duplicate.clone(),
                });
                inverse.push(BoardMutation::SetSeparatorDeletion {
                    separator_id: duplicate.id,
                    deleted_at: Some(at),
                    position,
                });
            }
            _ => return Err(ApplicationError::InvalidState),
        }
    }
    inverse.reverse();
    let operation = batch_operation(
        state,
        operation_id,
        BoardOperationKind::Duplicate,
        forward,
        inverse,
        at,
    )?;
    state.record_board_operation(&operation)?;
    state.focused_item = duplicate_ids.first().copied();
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

pub(in crate::application) fn delete_items(
    state: &mut AppState,
    operation_id: OperationId,
    item_ids: &[BoardItemId],
    kind: BoardOperationKind,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    if item_ids.is_empty() || kind != BoardOperationKind::Delete {
        return Err(ApplicationError::InvalidState);
    }
    let live = state.board.live_items();
    let selected = live
        .iter()
        .copied()
        .filter(|item| item_ids.contains(&item.id()))
        .collect::<Vec<_>>();
    if !matches_exact_item_order(&selected, item_ids) {
        return Err(ApplicationError::InvalidState);
    }
    let first_index = selected
        .first()
        .and_then(|item| usize::try_from(item.position().get()).ok())
        .ok_or(ApplicationError::InvalidState)?;
    let focus_removed = state
        .focused_item
        .is_some_and(|focus| item_ids.contains(&focus));
    let forward = selected
        .iter()
        .rev()
        .map(|item| item_deletion(*item, Some(at)))
        .collect();
    let inverse = selected
        .iter()
        .map(|item| item_deletion(*item, None))
        .collect();
    let operation = batch_operation(state, operation_id, kind, forward, inverse, at)?;
    state.record_board_operation(&operation)?;
    if focus_removed {
        let live = state.board.live_items();
        state.focused_item = live
            .get(first_index)
            .or_else(|| first_index.checked_sub(1).and_then(|index| live.get(index)))
            .map(|item| item.id());
    }
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

fn matches_exact_item_order(
    selected: &[crate::domain::BoardItemRef<'_>],
    requested: &[BoardItemId],
) -> bool {
    selected.len() == requested.len()
        && selected
            .iter()
            .map(|item| item.id())
            .eq(requested.iter().copied())
}

fn item_deletion(
    item: crate::domain::BoardItemRef<'_>,
    deleted_at: Option<Timestamp>,
) -> BoardMutation {
    match item {
        crate::domain::BoardItemRef::Thought(thought) => deletion(thought, deleted_at),
        crate::domain::BoardItemRef::Separator(separator) => BoardMutation::SetSeparatorDeletion {
            separator_id: separator.id,
            deleted_at,
            position: separator.position,
        },
    }
}

fn validate_deletion(thought_ids: &[ThoughtId], kind: BoardOperationKind) -> ApplicationResult<()> {
    if thought_ids.is_empty()
        || !matches!(
            kind,
            BoardOperationKind::Delete
                | BoardOperationKind::Cut
                | BoardOperationKind::SubmitAndRemove
                | BoardOperationKind::TransferAndRemove
        )
    {
        return Err(ApplicationError::InvalidState);
    }
    Ok(())
}

fn selected_thoughts(
    state: &AppState,
    thought_ids: &[ThoughtId],
) -> ApplicationResult<Vec<Thought>> {
    let selected = state
        .board
        .live_thoughts()
        .into_iter()
        .filter(|thought| thought_ids.contains(&thought.id))
        .cloned()
        .collect::<Vec<_>>();
    if selected.len() != thought_ids.len() || selected.is_empty() {
        return Err(ApplicationError::InvalidState);
    }
    Ok(selected)
}

fn deletion(thought: &Thought, deleted_at: Option<Timestamp>) -> BoardMutation {
    BoardMutation::SetDeletion {
        thought_id: thought.id,
        deleted_at,
        position: thought.position,
    }
}

const fn presentation_mutation(
    thought_id: ThoughtId,
    presentation: ThoughtPresentation,
) -> BoardMutation {
    BoardMutation::SetPresentation {
        thought_id,
        presentation,
    }
}

fn batch_operation(
    state: &mut AppState,
    operation_id: OperationId,
    kind: BoardOperationKind,
    forward: Vec<BoardMutation>,
    inverse: Vec<BoardMutation>,
    at: Timestamp,
) -> ApplicationResult<BoardOperation> {
    Ok(BoardOperation {
        id: operation_id,
        session_id: state.board.session.id,
        sequence: state.next_sequence()?,
        kind,
        forward: BoardMutation::Batch { mutations: forward },
        inverse: BoardMutation::Batch { mutations: inverse },
        created_at: at,
    })
}
