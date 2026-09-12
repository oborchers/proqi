//! Exhaustive undo ownership for every normalized semantic mutation.

use super::Action;

/// Canonical history contract shared by semantic mutations and active UI owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UndoContract {
    /// Restart-safe session Board operation.
    DurableBoard,
    /// Restart-safe active-thought Editor revision.
    DurableEditor,
    /// Restart-safe cross-session Browser administration operation.
    DurableBrowser,
    /// First content-producing Compose intention handed to a Board Create.
    ComposeHandoff,
    /// In-memory text history for the lifetime of one editable field.
    LocalText,
    /// Browser query text when present, otherwise durable Browser history.
    BrowserTextThenDurable,
    /// Undo or redo whose concrete durable owner is carried by the action.
    ContextualDurableMove,
    /// External intent whose successful local removal belongs to Board history.
    BoardAfterExternalIntent,
    /// External completion that may finish an already admitted Board operation.
    BoardAfterExternalCompletion,
    /// An external effect with no reversible local counterpart.
    IrreversibleExternal,
    /// Navigation, presentation, or a blocking owner with no history entry.
    Unavailable,
    /// Persistence bookkeeping that completes an already classified intention.
    Internal,
}

impl UndoContract {
    const fn starts_durable_mutation(self) -> bool {
        matches!(
            self,
            Self::DurableBoard
                | Self::DurableEditor
                | Self::DurableBrowser
                | Self::ComposeHandoff
                | Self::ContextualDurableMove
                | Self::BoardAfterExternalIntent
        )
    }
}

impl Action {
    /// Classify every reducer input by its one truthful undo contract.
    #[must_use]
    pub(crate) const fn undo_contract(&self) -> UndoContract {
        match self {
            Self::RenameSession { .. } => UndoContract::DurableBrowser,
            Self::CreateThought { .. }
            | Self::CreateOwnedThought(_)
            | Self::PasteAsThought { .. }
            | Self::ReflowThought(_)
            | Self::SplitThought { .. }
            | Self::ExtractThought { .. }
            | Self::MergeThoughts { .. }
            | Self::DeleteThought { .. }
            | Self::DeleteThoughts { .. }
            | Self::StageSubmissionRemoval { .. }
            | Self::MoveThought { .. }
            | Self::SetPresentation { .. }
            | Self::SetPresentationMany { .. }
            | Self::DuplicateThoughts { .. } => UndoContract::DurableBoard,
            Self::CreateComposeThought { .. } => UndoContract::ComposeHandoff,
            Self::EditThought { .. } | Self::EditOwnedThought(_) => UndoContract::DurableEditor,
            Self::Undo { .. } | Self::Redo { .. } => UndoContract::ContextualDurableMove,
            Self::CutThoughts { .. } => UndoContract::BoardAfterExternalIntent,
            Self::ClipboardResult { .. } => UndoContract::BoardAfterExternalCompletion,
            Self::CopyThoughts { .. } => UndoContract::IrreversibleExternal,
            Self::FocusThought(_)
            | Self::EnterEdit(_)
            | Self::EnterCompose
            | Self::ExitCompose
            | Self::ExitEdit
            | Self::BeginSubmission { .. }
            | Self::EndSubmission { .. } => UndoContract::Unavailable,
            Self::PersistenceCommitted(_)
            | Self::PersistenceFailed { .. }
            | Self::RetryPersistence(_) => UndoContract::Internal,
        }
    }

    /// Whether this input begins a new durable mutation or history move.
    #[must_use]
    pub(crate) const fn mutates_durable_state(&self) -> bool {
        self.undo_contract().starts_durable_mutation()
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, UndoContract};
    use crate::{
        adapters::memory::FakeIdGenerator,
        domain::{OperationSequence, TextPosition, Timestamp, UndoScope},
        ports::environment::IdGenerator as _,
    };

    fn assert_contract(action: &Action, expected: UndoContract, starts_durable: bool) {
        assert_eq!(action.undo_contract(), expected, "{action:?}");
        assert_eq!(action.mutates_durable_state(), starts_durable, "{action:?}");
    }

    #[test]
    fn browser_board_and_compose_actions_have_distinct_durable_owners() {
        let mut ids = FakeIdGenerator::new(1_800_500_000_000);
        let thought_id = ids.thought_id();
        let operation_id = ids.operation_id();
        let at = Timestamp::from_millis(1);
        assert_contract(
            &Action::RenameSession {
                operation_id,
                name: Some("name".to_owned()),
                at,
            },
            UndoContract::DurableBrowser,
            true,
        );
        assert_contract(
            &Action::CreateThought {
                thought_id,
                operation_id,
                content: "content".to_owned(),
                annotations: Vec::new(),
                insertion_index: None,
                at,
            },
            UndoContract::DurableBoard,
            true,
        );
        assert_contract(
            &Action::CreateComposeThought {
                thought_id,
                operation_id: ids.operation_id(),
                content: "c".to_owned(),
                annotations: Vec::new(),
                cursor: TextPosition::new(0, 1),
                selection_anchor: None,
                preserve_owned: false,
                at,
            },
            UndoContract::ComposeHandoff,
            true,
        );
    }

    #[test]
    fn editor_and_history_moves_keep_their_exact_contracts() {
        let mut ids = FakeIdGenerator::new(1_800_510_000_000);
        let at = Timestamp::from_millis(1);
        assert_contract(
            &Action::EditThought {
                thought_id: ids.thought_id(),
                revision_id: ids.revision_id(),
                before_content: String::new(),
                after_content: "content".to_owned(),
                before_annotations: Vec::new(),
                after_annotations: Vec::new(),
                before_cursor: TextPosition::new(0, 0),
                after_cursor: TextPosition::new(0, 7),
                at,
            },
            UndoContract::DurableEditor,
            true,
        );
        assert_contract(
            &Action::Undo {
                operation_id: ids.operation_id(),
                scope: UndoScope::Board,
                at,
            },
            UndoContract::ContextualDurableMove,
            true,
        );
    }

    #[test]
    fn external_intent_completion_and_effect_have_truthful_contracts() {
        let mut ids = FakeIdGenerator::new(1_800_520_000_000);
        let request_id = ids.request_id();
        let thought_id = ids.thought_id();
        assert_contract(
            &Action::CutThoughts {
                request_id,
                operation_id: ids.operation_id(),
                thought_ids: vec![thought_id],
                at: Timestamp::from_millis(1),
            },
            UndoContract::BoardAfterExternalIntent,
            true,
        );
        assert_contract(
            &Action::ClipboardResult {
                request_id,
                result: Ok(()),
            },
            UndoContract::BoardAfterExternalCompletion,
            false,
        );
        assert_contract(
            &Action::CopyThoughts {
                request_id: ids.request_id(),
                thought_ids: vec![thought_id],
            },
            UndoContract::IrreversibleExternal,
            false,
        );
    }

    #[test]
    fn navigation_and_persistence_bookkeeping_create_no_history() {
        assert_contract(
            &Action::FocusThought(None),
            UndoContract::Unavailable,
            false,
        );
        assert_contract(
            &Action::PersistenceCommitted(OperationSequence::new(1)),
            UndoContract::Internal,
            false,
        );
    }
}
