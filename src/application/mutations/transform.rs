//! Atomic split, extract, and merge thought transformations.

use std::ops::Range;

use super::{
    AppState, ApplicationError, ApplicationResult, BoardMutation, BoardOperation,
    BoardOperationKind, Effect, OperationId, Thought, ThoughtId, ThoughtPosition, Timestamp,
};
use crate::domain::{
    ContentAnnotation, extract_annotations, merge_annotations, partition_annotations,
};

#[derive(Clone)]
pub(in crate::application) struct ExactSource {
    pub(in crate::application) thought_id: ThoughtId,
    pub(in crate::application) content: String,
    pub(in crate::application) annotations: Vec<ContentAnnotation>,
}

pub(in crate::application) fn split_thought(
    state: &mut AppState,
    operation_id: OperationId,
    new_thought_id: ThoughtId,
    source: &ExactSource,
    at_byte: usize,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let thought = exact_source(state, source)?.clone();
    let (left_annotations, right_annotations) =
        partition_annotations(&thought.content, &thought.annotations, at_byte)?;
    let left = thought
        .content
        .get(..at_byte)
        .ok_or(ApplicationError::InvalidState)?
        .to_owned();
    let right = thought
        .content
        .get(at_byte..)
        .ok_or(ApplicationError::InvalidState)?
        .to_owned();
    transform_into_neighbor(
        state,
        operation_id,
        new_thought_id,
        thought,
        left,
        left_annotations,
        right,
        right_annotations,
        BoardOperationKind::Split,
        at,
    )
}

pub(in crate::application) fn extract_thought(
    state: &mut AppState,
    operation_id: OperationId,
    new_thought_id: ThoughtId,
    source: &ExactSource,
    range: Range<usize>,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let thought = exact_source(state, source)?.clone();
    let (remaining_annotations, extracted_annotations) =
        extract_annotations(&thought.content, &thought.annotations, range.clone())?;
    let prefix = thought
        .content
        .get(..range.start)
        .ok_or(ApplicationError::InvalidState)?;
    let extracted = thought
        .content
        .get(range.clone())
        .ok_or(ApplicationError::InvalidState)?
        .to_owned();
    let suffix = thought
        .content
        .get(range.end..)
        .ok_or(ApplicationError::InvalidState)?;
    let remaining = [prefix, suffix].concat();
    transform_into_neighbor(
        state,
        operation_id,
        new_thought_id,
        thought,
        remaining,
        remaining_annotations,
        extracted,
        extracted_annotations,
        BoardOperationKind::Extract,
        at,
    )
}

pub(in crate::application) fn merge_thoughts(
    state: &mut AppState,
    operation_id: OperationId,
    thought_ids: &[ThoughtId],
    expected_sources: &[Thought],
    separator: &str,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let selected = contiguous_sources(state, thought_ids)?;
    if selected != expected_sources {
        let changed = thought_ids
            .iter()
            .zip(expected_sources)
            .zip(&selected)
            .find_map(|((id, expected), current)| (expected != current).then_some(*id))
            .or_else(|| thought_ids.first().copied())
            .ok_or(ApplicationError::InvalidState)?;
        return Err(ApplicationError::ContentConflict(changed));
    }
    let first = selected[0].clone();
    let merged_content = selected
        .iter()
        .map(|thought| thought.content.as_str())
        .collect::<Vec<_>>()
        .join(separator);
    let merged_annotations = merge_annotations(
        selected
            .iter()
            .map(|thought| (thought.content.as_str(), thought.annotations.as_slice())),
        separator,
    )?;
    let mut forward = vec![replacement(
        &first,
        merged_content.clone(),
        merged_annotations.clone(),
    )];
    forward.extend(
        selected[1..]
            .iter()
            .rev()
            .map(|thought| deletion(thought, at)),
    );
    let mut inverse = selected[1..]
        .iter()
        .map(|thought| restoration(thought, at))
        .collect::<Vec<_>>();
    inverse.push(replacement_values(
        first.id,
        merged_content,
        merged_annotations,
        first.content.clone(),
        first.annotations.clone(),
    ));
    let operation = operation(
        state,
        operation_id,
        BoardOperationKind::Merge,
        forward,
        inverse,
        at,
    )?;
    record_transform(state, &operation, thought_ids)?;
    state.focused_thought = Some(first.id);
    state.mode = crate::application::InteractionMode::Board;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

#[expect(
    clippy::too_many_arguments,
    reason = "the exact two resulting thought snapshots form one atomic transformation"
)]
fn transform_into_neighbor(
    state: &mut AppState,
    operation_id: OperationId,
    new_thought_id: ThoughtId,
    source: Thought,
    source_content: String,
    source_annotations: Vec<ContentAnnotation>,
    neighbor_content: String,
    neighbor_annotations: Vec<ContentAnnotation>,
    kind: BoardOperationKind,
    at: Timestamp,
) -> ApplicationResult<Vec<Effect>> {
    let insertion = usize::try_from(source.position.get())
        .map_err(|_| ApplicationError::InvalidState)?
        .saturating_add(1);
    let mut neighbor = Thought::new(
        new_thought_id,
        source.session_id,
        neighbor_content,
        ThoughtPosition::new(super::position_u32(insertion)?),
        at,
    );
    neighbor.presentation = source.presentation;
    neighbor.set_annotations(neighbor_annotations)?;
    let forward = vec![
        replacement(&source, source_content.clone(), source_annotations.clone()),
        BoardMutation::AddThought {
            thought: neighbor.clone(),
        },
    ];
    let inverse = vec![
        deletion(&neighbor, at),
        replacement_values(
            source.id,
            source_content,
            source_annotations,
            source.content,
            source.annotations,
        ),
    ];
    let operation = operation(state, operation_id, kind, forward, inverse, at)?;
    record_transform(state, &operation, &[source.id])?;
    state.focused_thought = Some(new_thought_id);
    state.mode = crate::application::InteractionMode::Edit {
        thought_id: new_thought_id,
    };
    state.insertion_index = insertion + 1;
    Ok(vec![Effect::CommitBoardOperation(operation)])
}

fn exact_source<'a>(state: &'a AppState, source: &ExactSource) -> ApplicationResult<&'a Thought> {
    let thought = state.live_thought(source.thought_id)?;
    if thought.content != source.content || thought.annotations != source.annotations {
        return Err(ApplicationError::ContentConflict(source.thought_id));
    }
    Ok(thought)
}

fn contiguous_sources(
    state: &AppState,
    thought_ids: &[ThoughtId],
) -> ApplicationResult<Vec<Thought>> {
    if thought_ids.len() < 2 {
        return Err(ApplicationError::InvalidState);
    }
    let selected = state
        .board
        .live_thoughts()
        .into_iter()
        .filter(|thought| thought_ids.contains(&thought.id))
        .cloned()
        .collect::<Vec<_>>();
    let ordered = selected
        .iter()
        .map(|thought| thought.id)
        .collect::<Vec<_>>();
    let contiguous = selected
        .windows(2)
        .all(|pair| pair[1].position.get() == pair[0].position.get().saturating_add(1));
    if ordered != thought_ids || !contiguous {
        return Err(ApplicationError::NoncontiguousSelection);
    }
    Ok(selected)
}

fn replacement(
    source: &Thought,
    content: String,
    annotations: Vec<ContentAnnotation>,
) -> BoardMutation {
    replacement_values(
        source.id,
        source.content.clone(),
        source.annotations.clone(),
        content,
        annotations,
    )
}

fn replacement_values(
    thought_id: ThoughtId,
    before_content: String,
    before_annotations: Vec<ContentAnnotation>,
    after_content: String,
    after_annotations: Vec<ContentAnnotation>,
) -> BoardMutation {
    BoardMutation::ReplaceContent {
        thought_id,
        before_content,
        before_annotations,
        after_content,
        after_annotations,
    }
}

fn deletion(thought: &Thought, at: Timestamp) -> BoardMutation {
    BoardMutation::SetDeletionExact {
        thought_id: thought.id,
        expected_content: thought.content.clone(),
        expected_annotations: thought.annotations.clone(),
        expected_deleted_at: None,
        expected_position: thought.position,
        deleted_at: Some(at),
        position: thought.position,
    }
}

fn restoration(thought: &Thought, deleted_at: Timestamp) -> BoardMutation {
    BoardMutation::SetDeletionExact {
        thought_id: thought.id,
        expected_content: thought.content.clone(),
        expected_annotations: thought.annotations.clone(),
        expected_deleted_at: Some(deleted_at),
        expected_position: thought.position,
        deleted_at: None,
        position: thought.position,
    }
}

fn operation(
    state: &AppState,
    id: OperationId,
    kind: BoardOperationKind,
    forward: Vec<BoardMutation>,
    inverse: Vec<BoardMutation>,
    at: Timestamp,
) -> ApplicationResult<BoardOperation> {
    Ok(BoardOperation {
        id,
        session_id: state.board.session.id,
        sequence: state.next_sequence()?,
        kind,
        forward: BoardMutation::Batch { mutations: forward },
        inverse: BoardMutation::Batch { mutations: inverse },
        created_at: at,
    })
}

fn record_transform(
    state: &mut AppState,
    operation: &BoardOperation,
    edited: &[ThoughtId],
) -> ApplicationResult<()> {
    state.record_board_operation(operation)?;
    for thought_id in edited {
        if let Some(history) = state.editor_histories.get_mut(thought_id) {
            history.revisions.truncate(history.cursor);
        }
    }
    Ok(())
}
