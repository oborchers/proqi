//! Durable idempotency matching for owner-control requests.

mod replacement;

use crate::{
    domain::{BoardMutation, BoardOperationKind, SessionId},
    ports::{
        control::{ControlMutation, ControlReceipt},
        store::StoredOperationRequest,
    },
};

/// Result of comparing one requested operation with its durable identity.
pub(crate) enum ControlReplay {
    /// The exact mutation was already committed.
    Accepted(ControlReceipt),
    /// The operation identity belongs to another semantic request.
    Conflict,
}

/// Validate an operation replay without applying it to current state again.
pub(crate) fn match_control_replay(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> ControlReplay {
    let receipt = match existing {
        StoredOperationRequest::Board { receipt, .. }
        | StoredOperationRequest::HistoryMove { receipt, .. }
        | StoredOperationRequest::Revision { receipt, .. }
        | StoredOperationRequest::Compacted { receipt, .. } => *receipt,
    };
    let Some(identity) = mutation.durable_identity() else {
        return ControlReplay::Conflict;
    };
    if receipt.identity != identity {
        return ControlReplay::Conflict;
    }
    let thought_id = match mutation {
        ControlMutation::Add { thought_id, .. }
        | ControlMutation::PreserveAdd { thought_id, .. }
            if matches_add(existing, session_id, mutation) =>
        {
            Some(*thought_id)
        }
        ControlMutation::Delete { thought_id, .. }
            if matches_delete(existing, session_id, *thought_id) =>
        {
            Some(*thought_id)
        }
        ControlMutation::Move { thought_id, .. }
            if matches_move(existing, session_id, mutation) =>
        {
            Some(*thought_id)
        }
        ControlMutation::SetCollapsed { thought_id, .. }
            if matches_collapse(existing, session_id, mutation) =>
        {
            Some(*thought_id)
        }
        ControlMutation::History { scope, undo, .. }
            if matches_history(existing, session_id, *scope, *undo) =>
        {
            None
        }
        ControlMutation::Replace { thought_id, .. }
            if replacement::matches(existing, session_id, mutation) =>
        {
            Some(*thought_id)
        }
        _ => return ControlReplay::Conflict,
    };
    let mut durable = receipt;
    durable.idempotent_replay = true;
    ControlReplay::Accepted(ControlReceipt {
        thought_id,
        durable,
    })
}

fn matches_collapse(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let (
        StoredOperationRequest::Board { operation, .. },
        ControlMutation::SetCollapsed {
            thought_id,
            collapsed,
            ..
        },
    ) = (existing, mutation)
    else {
        return false;
    };
    operation.session_id == session_id
        && operation.kind == BoardOperationKind::Collapse
        && matches!(
            &operation.forward,
            BoardMutation::SetPresentation {
                thought_id: stored,
                presentation,
            } if stored == thought_id
                && presentation.is_collapsed() == *collapsed
        )
}

fn matches_add(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    let Some((thought_id, content, annotations, position)) = add_parts(mutation) else {
        return false;
    };
    if let StoredOperationRequest::Compacted { replay, .. } = existing {
        let crate::ports::store::CompactedOperationRequest::Add {
            session_id: stored_session,
            thought_id: stored_thought,
            payload_digest,
            position: stored_position,
        } = replay
        else {
            return false;
        };
        return crate::ports::store::thought_payload_digest(content, annotations).is_ok_and(
            |digest| {
                *stored_session == session_id
                    && stored_thought == thought_id
                    && *payload_digest == digest
                    && position.is_none_or(|value| value == *stored_position)
            },
        );
    }
    let (StoredOperationRequest::Board { operation, .. }, _) = (existing, mutation) else {
        return false;
    };
    operation.session_id == session_id
        && operation.kind == BoardOperationKind::Create
        && matches!(
            &operation.forward,
            BoardMutation::AddThought { thought }
                if thought.id == *thought_id
                    && thought.content == *content
                    && thought.annotations == *annotations
                    && position.is_none_or(|value| {
                        u32::try_from(value).ok() == Some(thought.position.get())
                    })
        )
}

fn add_parts(
    mutation: &ControlMutation,
) -> Option<(
    &crate::domain::ThoughtId,
    &str,
    &[crate::domain::ContentAnnotation],
    &Option<usize>,
)> {
    match mutation {
        ControlMutation::Add {
            thought_id,
            content,
            annotations,
            position,
            ..
        }
        | ControlMutation::PreserveAdd {
            thought_id,
            content,
            annotations,
            position,
            ..
        } => Some((thought_id, content, annotations, position)),
        _ => None,
    }
}

fn matches_delete(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    thought_id: crate::domain::ThoughtId,
) -> bool {
    if let StoredOperationRequest::Compacted { replay, .. } = existing {
        return matches!(
            replay,
            crate::ports::store::CompactedOperationRequest::Delete {
                session_id: stored_session,
                thought_id: stored_thought,
            } if *stored_session == session_id && *stored_thought == thought_id
        );
    }
    let StoredOperationRequest::Board { operation, .. } = existing else {
        return false;
    };
    operation.session_id == session_id
        && operation.kind == BoardOperationKind::Delete
        && matches!(
            &operation.forward,
            BoardMutation::SetDeletion {
                thought_id: stored,
                deleted_at: Some(_),
                ..
            } if *stored == thought_id
        )
}

fn matches_move(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    mutation: &ControlMutation,
) -> bool {
    if let StoredOperationRequest::Compacted { replay, .. } = existing {
        let ControlMutation::Move {
            thought_id,
            position,
            ..
        } = mutation
        else {
            return false;
        };
        return matches!(
            replay,
            crate::ports::store::CompactedOperationRequest::Move {
                session_id: stored_session,
                thought_id: stored_thought,
                position: stored_position,
            } if *stored_session == session_id
                && stored_thought == thought_id
                && stored_position == position
        );
    }
    let (
        StoredOperationRequest::Board { operation, .. },
        ControlMutation::Move {
            thought_id,
            position,
            ..
        },
    ) = (existing, mutation)
    else {
        return false;
    };
    operation.session_id == session_id
        && operation.kind == BoardOperationKind::Reorder
        && matches!(
            &operation.forward,
            BoardMutation::MoveThought {
                thought_id: stored,
                to,
                ..
            } if stored == thought_id && usize::try_from(to.get()).ok() == Some(*position)
        )
}

fn matches_history(
    existing: &StoredOperationRequest,
    session_id: SessionId,
    scope: crate::domain::UndoScope,
    undo: bool,
) -> bool {
    match existing {
        StoredOperationRequest::HistoryMove {
            session_id: stored_session,
            scope: stored_scope,
            undo: stored_undo,
            ..
        }
        | StoredOperationRequest::Compacted {
            replay:
                crate::ports::store::CompactedOperationRequest::History {
                    session_id: stored_session,
                    scope: stored_scope,
                    undo: stored_undo,
                },
            ..
        } => *stored_session == session_id && *stored_scope == scope && *stored_undo == undo,
        StoredOperationRequest::Board { .. }
        | StoredOperationRequest::Revision { .. }
        | StoredOperationRequest::Compacted { .. } => false,
    }
}

#[cfg(test)]
mod tests;
