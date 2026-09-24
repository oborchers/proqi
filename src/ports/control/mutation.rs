//! Mutation payloads for the active-owner control protocol.

use serde::{Deserialize, Serialize};

use crate::domain::{
    BoardItemId, ContentAnnotation, ContentAnnotationKind, OperationId, RequestId, RevisionId,
    SeparatorId, ThoughtId, ThoughtName, UndoScope,
};

use super::super::store::DurableIdentity;
use super::super::update::{UpdatePrepareRequest, UpdateQuiesceRequest, UpdateRestartRequest};
use super::UPDATE_MUTATION_MINIMUM_PROTOCOL;
use crate::ports::transfer::TransferItem;

/// One mutation routed to the process owning a session reducer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mutation")]
pub enum ControlMutation {
    /// Rename or clear the active session through its owner.
    RenameSession {
        /// Durable Browser operation identity.
        operation_id: OperationId,
        /// Replacement name, or `None` to clear it.
        name: Option<String>,
    },
    /// Flush pending editor work before an active-session CLI read.
    Sync,
    /// Replace exact thought content as one persistent editor revision.
    Replace {
        /// Durable editor revision identity.
        revision_id: RevisionId,
        /// Thought to replace.
        thought_id: ThoughtId,
        /// Required SHA-256 of current content, omitted only for explicit force.
        expected_digest: Option<[u8; 32]>,
        /// Exact replacement content.
        content: String,
    },
    /// Set one thought's durable collapse state.
    SetCollapsed {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Thought to update.
        thought_id: ThoughtId,
        /// Exact replacement state.
        collapsed: bool,
    },
    /// Create one exact-content thought.
    Add {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Deterministic thought identity associated with this request.
        thought_id: ThoughtId,
        /// Exact content, including line endings.
        content: String,
        /// Durable presentation metadata, available from protocol version 2.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<ContentAnnotation>,
        /// Optional zero-based insertion position.
        position: Option<usize>,
    },
    /// Create one thought whose content, validated metadata, and optional name are exact.
    ///
    /// Cross-session transfer preserves an existing thought through this request,
    /// and named CLI creation uses it so content, name, and position commit as one
    /// Board operation.
    PreserveAdd {
        /// Durable destination operation identity.
        operation_id: OperationId,
        /// Deterministic destination thought identity.
        thought_id: ThoughtId,
        /// Exact canonical source content.
        content: String,
        /// Existing validated presentation metadata preserved without re-authoring it.
        annotations: Vec<ContentAnnotation>,
        /// Optional organizational metadata preserved outside authored content.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<ThoughtName>,
        /// Optional zero-based destination position.
        position: Option<usize>,
    },
    /// Preserve an exact selected cohort in one destination transaction.
    PreserveAddMany {
        /// One idempotent destination operation identity.
        operation_id: OperationId,
        /// Ordered exact source snapshots and stable destination identities.
        items: Vec<TransferItem>,
    },
    /// Set or clear one thought's optional organizational name.
    RenameThought {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Thought to update.
        thought_id: ThoughtId,
        /// Replacement name, or `None` to clear it.
        name: Option<ThoughtName>,
    },
    /// Insert one payload-free separator into the shared Board order.
    InsertSeparator {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Deterministic separator identity associated with this request.
        separator_id: SeparatorId,
        /// Optional zero-based shared Board position.
        position: Option<usize>,
    },
    /// Soft-delete one or more typed Board items.
    DeleteItems {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Exact source identities in Board order.
        item_ids: Vec<BoardItemId>,
    },
    /// Move one typed Board item.
    MoveItem {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Exact source identity.
        item_id: BoardItemId,
        /// Zero-based target in shared Board order.
        position: usize,
    },
    /// Duplicate one or more typed Board items.
    DuplicateItems {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Exact source identities in Board order.
        item_ids: Vec<BoardItemId>,
        /// Fresh typed identities paired with the sources.
        duplicate_ids: Vec<BoardItemId>,
    },
    /// Split one exact thought at a UTF-8 byte boundary.
    SplitThought {
        /// Durable Board operation identity.
        operation_id: OperationId,
        /// Exact source thought.
        thought_id: ThoughtId,
        /// Deterministic identity for the right-hand thought.
        new_thought_id: ThoughtId,
        /// Required SHA-256 of current source content.
        expected_digest: [u8; 32],
        /// UTF-8 byte boundary dividing left and right content.
        at_byte: usize,
    },
    /// Extract one exact nonempty UTF-8 byte range into a neighboring thought.
    ExtractThought {
        /// Durable Board operation identity.
        operation_id: OperationId,
        /// Exact source thought.
        thought_id: ThoughtId,
        /// Deterministic identity for the extracted thought.
        new_thought_id: ThoughtId,
        /// Required SHA-256 of current source content.
        expected_digest: [u8; 32],
        /// Inclusive UTF-8 byte boundary where extraction starts.
        start_byte: usize,
        /// Exclusive UTF-8 byte boundary where extraction ends.
        end_byte: usize,
    },
    /// Merge exact Board-contiguous thoughts.
    MergeThoughts {
        /// Durable Board operation identity.
        operation_id: OperationId,
        /// Source thoughts in their exact shared Board order.
        thought_ids: Vec<ThoughtId>,
        /// Required SHA-256 values paired with the source thoughts.
        expected_digests: Vec<[u8; 32]>,
        /// Exact content inserted between source bodies.
        separator: String,
    },
    /// Clean one exact thought with Proqi's canonical spacing policy.
    ReflowThought {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Exact source thought.
        thought_id: ThoughtId,
        /// Required SHA-256 of current content.
        expected_digest: [u8; 32],
    },
    /// Soft-delete one thought.
    Delete {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Thought to delete.
        thought_id: ThoughtId,
    },
    /// Move one thought.
    Move {
        /// Durable board operation identity.
        operation_id: OperationId,
        /// Thought to move.
        thought_id: ThoughtId,
        /// Zero-based target position.
        position: usize,
    },
    /// Persistently move one history scope.
    History {
        /// Durable history operation identity.
        operation_id: OperationId,
        /// Board or editor scope.
        scope: UndoScope,
        /// Undo when true, redo otherwise.
        undo: bool,
    },
    /// Ask one live owner to flush and enter a bounded update barrier.
    UpdatePrepare {
        /// Shared all-session readiness request.
        request: UpdatePrepareRequest,
    },
    /// Release a previously prepared owner after cancellation or failure.
    UpdateRelease {
        /// Shared attempt identity.
        operation_id: RequestId,
    },
    /// Commit one prepared owner to irreversible schema quiescence.
    UpdateQuiesce {
        /// Exact installed target and shared attempt identity.
        request: UpdateQuiesceRequest,
    },
    /// Ask one prepared owner to clean up and replace itself.
    UpdateRestart {
        /// Verified installed version and shared attempt identity.
        request: UpdateRestartRequest,
    },
    /// Ask the exact live screenshot owner to schedule a verified graceful handoff.
    CaptureTakeover {
        /// Owner identity observed with the authoritative lock contention.
        expected_owner_instance_id: crate::domain::InstanceId,
        /// Process that will retry the authoritative lock.
        requester_instance_id: crate::domain::InstanceId,
        /// Screenshot takeover protocol required by the requester.
        capture_protocol: u32,
    },
}

impl ControlMutation {
    /// Durable idempotency identity carried by every mutation.
    #[must_use]
    pub const fn durable_operation_id(&self) -> Option<OperationId> {
        match self {
            Self::RenameSession { operation_id, .. }
            | Self::Add { operation_id, .. }
            | Self::PreserveAdd { operation_id, .. }
            | Self::PreserveAddMany { operation_id, .. }
            | Self::RenameThought { operation_id, .. }
            | Self::InsertSeparator { operation_id, .. }
            | Self::DeleteItems { operation_id, .. }
            | Self::MoveItem { operation_id, .. }
            | Self::DuplicateItems { operation_id, .. }
            | Self::SplitThought { operation_id, .. }
            | Self::ExtractThought { operation_id, .. }
            | Self::MergeThoughts { operation_id, .. }
            | Self::ReflowThought { operation_id, .. }
            | Self::Delete { operation_id, .. }
            | Self::Move { operation_id, .. }
            | Self::History { operation_id, .. }
            | Self::SetCollapsed { operation_id, .. } => Some(*operation_id),
            Self::UpdatePrepare { .. }
            | Self::Sync
            | Self::Replace { .. }
            | Self::UpdateRelease { .. }
            | Self::UpdateQuiesce { .. }
            | Self::UpdateRestart { .. }
            | Self::CaptureTakeover { .. } => None,
        }
    }

    /// Durable idempotency identity carried by a mutation.
    #[must_use]
    pub const fn durable_identity(&self) -> Option<DurableIdentity> {
        match self {
            Self::Replace { revision_id, .. } => Some(DurableIdentity::Revision(*revision_id)),
            _ => match self.durable_operation_id() {
                Some(operation_id) => Some(DurableIdentity::Operation(operation_id)),
                None => None,
            },
        }
    }

    /// Thought affected by this request, when applicable.
    #[must_use]
    pub const fn thought_id(&self) -> Option<ThoughtId> {
        match self {
            Self::Add { thought_id, .. }
            | Self::PreserveAdd { thought_id, .. }
            | Self::Delete { thought_id, .. }
            | Self::Move { thought_id, .. }
            | Self::Replace { thought_id, .. }
            | Self::SetCollapsed { thought_id, .. }
            | Self::RenameThought { thought_id, .. }
            | Self::SplitThought { thought_id, .. }
            | Self::ExtractThought { thought_id, .. }
            | Self::ReflowThought { thought_id, .. } => Some(*thought_id),
            Self::InsertSeparator { .. }
            | Self::PreserveAddMany { .. }
            | Self::DeleteItems { .. }
            | Self::MoveItem { .. }
            | Self::DuplicateItems { .. }
            | Self::MergeThoughts { .. }
            | Self::History { .. }
            | Self::RenameSession { .. }
            | Self::Sync
            | Self::UpdatePrepare { .. }
            | Self::UpdateRelease { .. }
            | Self::UpdateQuiesce { .. }
            | Self::UpdateRestart { .. }
            | Self::CaptureTakeover { .. } => None,
        }
    }

    /// Mixed Board identities returned after a durable mutation.
    #[must_use]
    pub fn item_ids(&self) -> Vec<BoardItemId> {
        match self {
            Self::InsertSeparator { separator_id, .. } => vec![(*separator_id).into()],
            Self::DeleteItems { item_ids, .. } => item_ids.clone(),
            Self::MoveItem { item_id, .. } => vec![*item_id],
            Self::DuplicateItems { duplicate_ids, .. } => duplicate_ids.clone(),
            Self::PreserveAddMany { items, .. } => items
                .iter()
                .map(|item| BoardItemId::Thought(item.destination_thought_id))
                .collect(),
            Self::SplitThought {
                thought_id,
                new_thought_id,
                ..
            }
            | Self::ExtractThought {
                thought_id,
                new_thought_id,
                ..
            } => vec![(*thought_id).into(), (*new_thought_id).into()],
            Self::MergeThoughts { thought_ids, .. } => thought_ids
                .iter()
                .copied()
                .map(BoardItemId::Thought)
                .collect(),
            Self::ReflowThought { thought_id, .. } => vec![(*thought_id).into()],
            _ => Vec::new(),
        }
    }

    /// Whether this mutation requires the annotation-aware protocol.
    #[must_use]
    pub fn requires_protocol_two(&self) -> bool {
        matches!(self, Self::Add { annotations, .. } if !annotations.is_empty())
    }

    /// Whether this mutation carries the invocation-reference annotation added in protocol six.
    #[must_use]
    pub fn requires_protocol_six(&self) -> bool {
        matches!(self, Self::Add { annotations, .. } if annotations.iter().any(|annotation| {
            matches!(annotation.kind, ContentAnnotationKind::InvocationReference { .. })
        }))
    }

    /// Whether this purpose-specific request preserves semantic inline metadata.
    #[must_use]
    pub fn requires_protocol_seven(&self) -> bool {
        matches!(self, Self::PreserveAdd { .. })
    }

    /// Oldest control protocol capable of representing this request.
    #[must_use]
    pub fn minimum_protocol(&self) -> u32 {
        if matches!(self, Self::PreserveAddMany { .. }) {
            12
        } else if matches!(
            self,
            Self::InsertSeparator { .. }
                | Self::DeleteItems { .. }
                | Self::MoveItem { .. }
                | Self::DuplicateItems { .. }
                | Self::SplitThought { .. }
                | Self::ExtractThought { .. }
                | Self::MergeThoughts { .. }
                | Self::ReflowThought { .. }
        ) {
            11
        } else if matches!(self, Self::RenameThought { .. })
            || matches!(self, Self::PreserveAdd { name: Some(_), .. })
        {
            10
        } else if matches!(self, Self::RenameSession { .. }) {
            9
        } else if matches!(self, Self::Add { annotations, .. } | Self::PreserveAdd { annotations, .. } if annotations.iter().any(|annotation| matches!(annotation.kind, ContentAnnotationKind::Attachment { .. })))
        {
            8
        } else if self.requires_protocol_seven() {
            7
        } else if self.requires_protocol_six() {
            6
        } else if matches!(self, Self::CaptureTakeover { .. }) {
            5
        } else if matches!(
            self,
            Self::Replace { .. } | Self::SetCollapsed { .. } | Self::Sync
        ) {
            4
        } else if matches!(
            self,
            Self::UpdatePrepare { .. }
                | Self::UpdateRelease { .. }
                | Self::UpdateQuiesce { .. }
                | Self::UpdateRestart { .. }
        ) {
            UPDATE_MUTATION_MINIMUM_PROTOCOL
        } else if self.requires_protocol_two() {
            2
        } else {
            1
        }
    }
}
