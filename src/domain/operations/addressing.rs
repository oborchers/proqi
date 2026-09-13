//! Thought identities addressed by structural mutations.

use super::{BoardMutation, ThoughtId};

impl BoardMutation {
    /// Whether this mutation addresses one thought identity.
    #[must_use]
    pub fn addresses(&self, thought_id: ThoughtId) -> bool {
        match self {
            Self::Batch { mutations } => mutations
                .iter()
                .any(|mutation| mutation.addresses(thought_id)),
            Self::AddThought { thought } | Self::AddThoughtFromCompose { thought, .. } => {
                thought.id == thought_id
            }
            Self::SetDeletion {
                thought_id: affected,
                ..
            }
            | Self::SetDeletionExact {
                thought_id: affected,
                ..
            }
            | Self::MoveThought {
                thought_id: affected,
                ..
            }
            | Self::ReplaceContent {
                thought_id: affected,
                ..
            }
            | Self::SetPresentation {
                thought_id: affected,
                ..
            }
            | Self::LegacySetCollapsed {
                thought_id: affected,
                ..
            } => *affected == thought_id,
        }
    }

    pub(super) fn collect_thought_ids(&self, thought_ids: &mut Vec<ThoughtId>) {
        match self {
            Self::Batch { mutations } => {
                for mutation in mutations {
                    mutation.collect_thought_ids(thought_ids);
                }
            }
            Self::AddThought { thought } | Self::AddThoughtFromCompose { thought, .. } => {
                push_distinct(thought_ids, thought.id);
            }
            Self::SetDeletion { thought_id, .. }
            | Self::SetDeletionExact { thought_id, .. }
            | Self::MoveThought { thought_id, .. }
            | Self::ReplaceContent { thought_id, .. }
            | Self::SetPresentation { thought_id, .. }
            | Self::LegacySetCollapsed { thought_id, .. } => {
                push_distinct(thought_ids, *thought_id);
            }
        }
    }
}

fn push_distinct(thought_ids: &mut Vec<ThoughtId>, thought_id: ThoughtId) {
    if !thought_ids.contains(&thought_id) {
        thought_ids.push(thought_id);
    }
}
