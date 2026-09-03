//! Exact source ownership for asynchronous clipboard operations.

use super::ClipboardIntent;
use crate::domain::{ContentAnnotation, OperationId, Thought, ThoughtId, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::application) struct ClipboardSource {
    pub(in crate::application) thought_id: ThoughtId,
    content: String,
    annotations: Vec<ContentAnnotation>,
}

impl ClipboardSource {
    pub(in crate::application) fn capture(thought: &Thought) -> Self {
        Self {
            thought_id: thought.id,
            content: thought.content.clone(),
            annotations: thought.annotations.clone(),
        }
    }

    pub(in crate::application) fn still_matches(&self, thought: &Thought) -> bool {
        thought.is_live()
            && thought.content == self.content
            && thought.annotations == self.annotations
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::application) struct PendingClipboard {
    pub(in crate::application) sources: Vec<ClipboardSource>,
    pub(in crate::application) intent: ClipboardIntent,
    pub(in crate::application) operation_id: Option<OperationId>,
    pub(in crate::application) at: Timestamp,
}
