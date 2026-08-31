//! Canonical bridge between UI ownership and reducer-owned application state.

use crate::{
    application::{Action, DurabilityState, Effect, EmptyBoardTransition, FailureCode, reduce},
    domain::OperationSequence,
    ports::environment::{Clock, IdGenerator},
    ui::PastePayload,
};

use super::{BoardApp, ComposePresentation, InsertionFocus, pending_types::EditFlush};

impl BoardApp {
    pub(super) fn flush_edit_boundary(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> EditFlush {
        let effects = self.flush_pending_edit(ids, clock);
        if self.pending_edit.is_some() {
            EditFlush::Blocked(effects)
        } else {
            EditFlush::Complete(effects)
        }
    }

    pub(super) fn request_quit_after_edit_flush(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => {
                self.request_quit();
                effects
            }
            EditFlush::Blocked(effects) => effects,
        }
    }

    /// Apply one ordered persistence acknowledgement to the reducer state.
    pub fn acknowledge_persistence(
        &mut self,
        sequence: OperationSequence,
        succeeded: bool,
    ) -> Vec<Effect> {
        self.acknowledge_persistence_result(
            sequence,
            succeeded.then_some(()).ok_or(FailureCode::StorageFailed),
        )
    }

    /// Apply a typed ordered persistence result and release durability-gated follow-up work.
    pub fn acknowledge_persistence_result(
        &mut self,
        sequence: OperationSequence,
        result: Result<(), FailureCode>,
    ) -> Vec<Effect> {
        let succeeded = result.is_ok();
        let failure = result.as_ref().err().copied();
        if !succeeded {
            self.quit = false;
        } else if self.pending_edit.is_some() {
            self.edit_generation = self.edit_generation.wrapping_add(1);
        }
        let action = if succeeded {
            Action::PersistenceCommitted(sequence)
        } else {
            Action::PersistenceFailed {
                sequence,
                code: result.err().unwrap_or(FailureCode::StorageFailed),
            }
        };
        let _effects = self.reduce(action);
        self.complete_deferred_submission_durability(failure)
    }

    pub(super) fn request_quit(&mut self) {
        if matches!(
            self.state.durability,
            DurabilityState::Failed { failed, .. }
                if self.recovery_exported_for != Some(failed)
        ) {
            self.set_error("retry the save or export recovery before quitting");
        } else {
            self.quit = true;
        }
    }

    pub(super) fn begin_insertion(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.state.board.live_thoughts().is_empty() {
            let effects = self.reduce(Action::EnterCompose);
            self.sync_editor_from_state();
            self.compose_presentation = ComposePresentation::Editor;
            self.layout = None;
            effects
        } else {
            self.create(PastePayload::text(String::new()), ids, clock)
        }
    }

    pub(super) fn expand_and_enter_edit(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        if self.submission_locked(thought_id) {
            self.set_warning("thought has a submission in progress");
            return Vec::new();
        }
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        self.layout = None;
        let effects = self.expand_thought(thought_id, ids, clock);
        self.enter_edit();
        effects
    }

    pub(super) fn enter_edit(&mut self) {
        self.insertion_focus = InsertionFocus::Inactive;
        self.edit_boundary = None;
        if let Some(thought_id) = self.state.focused_thought {
            if self.submission_locked(thought_id) {
                self.set_warning("thought has a submission in progress");
                return;
            }
            self.clear_board_selection();
            let _effects = self.reduce(Action::EnterEdit(thought_id));
            self.sync_editor_from_state();
        }
    }

    pub(super) fn reload_editor(&mut self) {
        self.editor = None;
        self.sync_editor_from_state();
    }

    pub(super) fn reduce(&mut self, action: Action) -> Vec<Effect> {
        self.try_reduce(action).unwrap_or_default()
    }

    fn try_reduce(&mut self, action: Action) -> Option<Vec<Effect>> {
        let may_change_attachments = Self::may_change_attachments(&action);
        match reduce(&mut self.state, action) {
            Ok(effects) => {
                self.finish_attachment_mutation(may_change_attachments);
                let order = self
                    .state
                    .board
                    .live_thoughts()
                    .into_iter()
                    .map(|thought| thought.id)
                    .collect::<Vec<_>>();
                self.selection.reconcile(&order);
                Some(effects)
            }
            Err(error) => {
                self.set_error(error.to_string());
                None
            }
        }
    }

    pub(super) fn reduce_with_empty_transition(
        &mut self,
        action: Action,
        transition: EmptyBoardTransition,
    ) -> Vec<Effect> {
        let was_nonempty = !self.state.board.live_thoughts().is_empty();
        let Some(effects) = self.try_reduce(action) else {
            return Vec::new();
        };
        if was_nonempty && self.state.board.live_thoughts().is_empty() {
            self.state.reconcile_empty_board(transition);
            if transition == EmptyBoardTransition::ComposeAfterLocalRemoval
                && matches!(
                    self.state.mode,
                    crate::application::InteractionMode::Compose
                )
            {
                self.compose_presentation = ComposePresentation::Prompt;
            }
            self.sync_editor_from_state();
        }
        effects
    }
}
