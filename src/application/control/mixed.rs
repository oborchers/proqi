//! Replay matching for mixed Board items and text transformations.

#[cfg(test)]
mod tests;

use crate::{
    domain::{BoardMutation, BoardOperationKind, SessionId},
    ports::{control::ControlMutation, store::StoredOperationRequest},
};

fn board_operation(
    existing: &StoredOperationRequest,
    session_id: SessionId,
) -> Option<&crate::domain::BoardOperation> {
    match existing {
        StoredOperationRequest::Board { operation, .. } if operation.session_id == session_id => {
            Some(operation)
        }
        _ => None,
    }
}

pub(super) fn matches_insert_separator(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let ControlMutation::InsertSeparator {
        separator_id,
        position,
        ..
    } = mutation
    else {
        return false;
    };
    board_operation(existing, session_id).is_some_and(|operation| {
        operation.kind == BoardOperationKind::InsertSeparator
            && matches!(
                &operation.forward,
                BoardMutation::AddSeparator { separator }
                    if separator.id == *separator_id
                        && position.is_none_or(|value| {
                            u32::try_from(value).ok() == Some(separator.position.get())
                        })
            )
    })
}

pub(super) fn matches_move_item(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let ControlMutation::MoveItem {
        item_id, position, ..
    } = mutation
    else {
        return false;
    };
    board_operation(existing, session_id).is_some_and(|operation| {
        operation.kind == BoardOperationKind::Reorder
            && match (&operation.forward, item_id) {
                (
                    BoardMutation::MoveThought { thought_id, to, .. },
                    crate::domain::BoardItemId::Thought(expected),
                ) => thought_id == expected && usize::try_from(to.get()).ok() == Some(*position),
                (
                    BoardMutation::MoveSeparator {
                        separator_id, to, ..
                    },
                    crate::domain::BoardItemId::Separator(expected),
                ) => separator_id == expected && usize::try_from(to.get()).ok() == Some(*position),
                _ => false,
            }
    })
}

pub(super) fn matches_delete_items(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let ControlMutation::DeleteItems { item_ids, .. } = mutation else {
        return false;
    };
    board_operation(existing, session_id).is_some_and(|operation| {
        operation.kind == BoardOperationKind::Delete
            && restored_item_ids(&operation.inverse) == *item_ids
    })
}

pub(super) fn matches_duplicate_items(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let ControlMutation::DuplicateItems {
        operation_id,
        item_ids,
        ..
    } = mutation
    else {
        return false;
    };
    let Ok(expected) = crate::application::derived_duplicate_item_ids(*operation_id, item_ids)
    else {
        return false;
    };
    board_operation(existing, session_id).is_some_and(|operation| {
        operation.kind == BoardOperationKind::Duplicate && duplicated_ids(existing) == expected
    })
}

pub(super) fn duplicated_ids(existing: &StoredOperationRequest) -> Vec<crate::domain::BoardItemId> {
    let StoredOperationRequest::Board { operation, .. } = existing else {
        return Vec::new();
    };
    mutation_items(&operation.forward, true)
}

fn restored_item_ids(mutation: &BoardMutation) -> Vec<crate::domain::BoardItemId> {
    mutation_items(mutation, false)
}

fn mutation_items(mutation: &BoardMutation, additions: bool) -> Vec<crate::domain::BoardItemId> {
    let mutations = match mutation {
        BoardMutation::Batch { mutations } => mutations.as_slice(),
        other => std::slice::from_ref(other),
    };
    mutations
        .iter()
        .filter_map(|mutation| match mutation {
            BoardMutation::AddThought { thought } if additions => Some(thought.id.into()),
            BoardMutation::AddSeparator { separator } if additions => Some(separator.id.into()),
            BoardMutation::SetDeletion {
                thought_id,
                deleted_at: None,
                ..
            } if !additions => Some((*thought_id).into()),
            BoardMutation::SetSeparatorDeletion {
                separator_id,
                deleted_at: None,
                ..
            } if !additions => Some((*separator_id).into()),
            _ => None,
        })
        .collect()
}

pub(super) fn matches_transform(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let Some(operation) = board_operation(existing, session_id) else {
        return false;
    };
    match mutation {
        ControlMutation::SplitThought { .. } => matches_split(operation, mutation),
        ControlMutation::ExtractThought { .. } => matches_extract(operation, mutation),
        ControlMutation::MergeThoughts { .. } => matches_merge(operation, mutation),
        ControlMutation::ReflowThought { .. } => matches_reflow(operation, mutation),
        _ => false,
    }
}

fn matches_split(operation: &crate::domain::BoardOperation, mutation: &ControlMutation) -> bool {
    let ControlMutation::SplitThought {
        thought_id,
        new_thought_id,
        expected_digest,
        at_byte,
        ..
    } = mutation
    else {
        return false;
    };
    operation.kind == BoardOperationKind::Split
        && split_parts(operation).is_some_and(|(source, before, left, created, right)| {
            source == *thought_id
                && created == *new_thought_id
                && digest(&before) == *expected_digest
                && before.get(..*at_byte) == Some(left.as_str())
                && before.get(*at_byte..) == Some(right.as_str())
        })
}

fn matches_extract(operation: &crate::domain::BoardOperation, mutation: &ControlMutation) -> bool {
    let ControlMutation::ExtractThought {
        thought_id,
        new_thought_id,
        expected_digest,
        start_byte,
        end_byte,
        ..
    } = mutation
    else {
        return false;
    };
    operation.kind == BoardOperationKind::Extract
        && split_parts(operation).is_some_and(|(source, before, remaining, created, extracted)| {
            source == *thought_id
                && created == *new_thought_id
                && digest(&before) == *expected_digest
                && before.get(*start_byte..*end_byte) == Some(extracted.as_str())
                && before
                    .get(..*start_byte)
                    .zip(before.get(*end_byte..))
                    .is_some_and(|(left, right)| [left, right].concat() == remaining)
        })
}

fn matches_merge(operation: &crate::domain::BoardOperation, mutation: &ControlMutation) -> bool {
    let ControlMutation::MergeThoughts {
        thought_ids,
        expected_digests,
        separator,
        ..
    } = mutation
    else {
        return false;
    };
    let Some(sources) =
        merge_sources(operation).filter(|_| operation.kind == BoardOperationKind::Merge)
    else {
        return false;
    };
    let ids = sources.iter().map(|source| source.0).collect::<Vec<_>>();
    let digests = sources
        .iter()
        .map(|source| digest(&source.1))
        .collect::<Vec<_>>();
    let merged = sources
        .iter()
        .map(|source| source.1.as_str())
        .collect::<Vec<_>>()
        .join(separator);
    ids == *thought_ids
        && digests == *expected_digests
        && replacement_after(&operation.forward).as_deref() == Some(merged.as_str())
}

fn matches_reflow(operation: &crate::domain::BoardOperation, mutation: &ControlMutation) -> bool {
    let ControlMutation::ReflowThought {
        thought_id,
        expected_digest,
        ..
    } = mutation
    else {
        return false;
    };
    operation.kind == BoardOperationKind::Reflow
        && replacement_parts(&operation.forward).is_some_and(
            |(stored, before, before_annotations, after, after_annotations)| {
                if stored != *thought_id || digest(&before) != *expected_digest {
                    return false;
                }
                crate::application::text_reflow::reflow(&before, &before_annotations).is_ok_and(
                    |projection| {
                        matches!(
                            projection.outcome,
                            crate::application::text_reflow::TextReflowOutcome::Changed {
                                content,
                                annotations,
                            } if content == after && annotations == after_annotations
                        )
                    },
                )
            },
        )
}

fn split_parts(
    operation: &crate::domain::BoardOperation,
) -> Option<(
    crate::domain::ThoughtId,
    String,
    String,
    crate::domain::ThoughtId,
    String,
)> {
    let BoardMutation::Batch { mutations } = &operation.forward else {
        return None;
    };
    let [
        BoardMutation::ReplaceContent {
            thought_id,
            before_content,
            after_content,
            ..
        },
        BoardMutation::AddThought { thought },
    ] = mutations.as_slice()
    else {
        return None;
    };
    Some((
        *thought_id,
        before_content.clone(),
        after_content.clone(),
        thought.id,
        thought.content.clone(),
    ))
}

fn merge_sources(
    operation: &crate::domain::BoardOperation,
) -> Option<Vec<(crate::domain::ThoughtId, String)>> {
    let BoardMutation::Batch { mutations } = &operation.inverse else {
        return None;
    };
    let mut sources = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            BoardMutation::SetDeletionExact {
                thought_id,
                expected_content,
                deleted_at: None,
                ..
            } => Some((*thought_id, expected_content.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let first = mutations.iter().find_map(|mutation| match mutation {
        BoardMutation::ReplaceContent {
            thought_id,
            after_content,
            ..
        } => Some((*thought_id, after_content.clone())),
        _ => None,
    })?;
    sources.insert(0, first);
    Some(sources)
}

fn replacement_after(mutation: &BoardMutation) -> Option<String> {
    let mutations = match mutation {
        BoardMutation::Batch { mutations } => mutations.as_slice(),
        other => std::slice::from_ref(other),
    };
    mutations.iter().find_map(|mutation| match mutation {
        BoardMutation::ReplaceContent { after_content, .. } => Some(after_content.clone()),
        _ => None,
    })
}

type ReplacementParts = (
    crate::domain::ThoughtId,
    String,
    Vec<crate::domain::ContentAnnotation>,
    String,
    Vec<crate::domain::ContentAnnotation>,
);

fn replacement_parts(mutation: &BoardMutation) -> Option<ReplacementParts> {
    let mutations = match mutation {
        BoardMutation::Batch { mutations } => mutations.as_slice(),
        other => std::slice::from_ref(other),
    };
    mutations.iter().find_map(|mutation| match mutation {
        BoardMutation::ReplaceContent {
            thought_id,
            before_content,
            before_annotations,
            after_content,
            after_annotations,
        } => Some((
            *thought_id,
            before_content.clone(),
            before_annotations.clone(),
            after_content.clone(),
            after_annotations.clone(),
        )),
        _ => None,
    })
}

fn digest(content: &str) -> [u8; 32] {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(content.as_bytes()).into()
}
