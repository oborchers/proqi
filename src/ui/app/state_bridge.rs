//! Canonical bridge between UI ownership and reducer-owned application state.

use crate::{
    application::{
        Action, DurabilityState, Effect, EmptyBoardTransition, FailureCode, InteractionMode, reduce,
    },
    domain::OperationSequence,
    ports::{
        editor::{CursorMovement, EditCommand},
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, ComposePresentation, EditorOwner, InsertionFocus, pending_types::EditFlush};

impl BoardApp {
    /// Rebuild the editor adapter when reducer state changes externally.
    pub fn sync_editor_from_state(&mut self) {
        let thought_id = match self.state.mode {
            InteractionMode::Board => {
                self.editor = None;
                return;
            }
            InteractionMode::Compose => {
                if !matches!(self.editor, Some((EditorOwner::Compose, _))) {
                    let mut editor = self.editor_factory.create("");
                    editor.set_viewport(self.viewport);
                    self.editor = Some((EditorOwner::Compose, editor));
                    self.compose_presentation = ComposePresentation::Prompt;
                }
                return;
            }
            InteractionMode::Edit { thought_id } => thought_id,
        };
        let Some(thought) = self.state.board.thought(thought_id) else {
            self.editor = None;
            return;
        };
        let content = thought.content.clone();
        let restored_state = self.state.restored_editor_state(thought_id);
        if let Some((EditorOwner::Thought(current), editor)) = &mut self.editor
            && *current == thought_id
        {
            if self.pending_edit.is_none() && editor.snapshot().content != content {
                let (cursor, anchor) = restored_state.unwrap_or_default();
                let _outcome = editor.replace_state(content, cursor, anchor);
            }
        } else {
            self.edit_owner_generation = self.edit_owner_generation.wrapping_add(1);
            let mut editor = self.editor_factory.create(&content);
            editor.set_viewport(self.viewport);
            if let Some((cursor, anchor)) = restored_state {
                let _outcome = editor.replace_state(content, cursor, anchor);
            } else {
                let _outcome = editor.apply(EditCommand::Move {
                    movement: CursorMovement::DocumentEnd,
                    extend_selection: false,
                });
            }
            self.editor = Some((EditorOwner::Thought(thought_id), editor));
        }
    }

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
        let action = if succeeded {
            Action::PersistenceCommitted(sequence)
        } else {
            Action::PersistenceFailed {
                sequence,
                code: result.err().unwrap_or(FailureCode::StorageFailed),
            }
        };
        let may_change_attachments = Self::may_change_attachments(&action);
        if reduce(&mut self.state, action).is_err() {
            return Vec::new();
        }
        self.finish_successful_reduce(may_change_attachments);
        self.acknowledge_first_control_focus(sequence, succeeded);
        if !succeeded {
            self.quit = false;
        } else if self.pending_edit.is_some() {
            self.edit_generation = self.edit_generation.wrapping_add(1);
        }
        if failure.is_some() {
            self.invalidate_palette();
            self.enter_storage_failure_state();
        } else {
            self.clear_storage_failure_status();
        }
        self.complete_deferred_submission_durability(failure)
    }

    pub(super) fn request_quit(&mut self) {
        if matches!(
            self.state.durability,
            DurabilityState::Failed { failed, .. }
                if self.recovery_exported_for != Some(failed)
        ) {
            self.set_storage_failure("retry the save or export recovery before quitting");
        } else {
            self.quit = true;
        }
    }

    pub(super) fn expand_and_enter_edit(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought_id) = self.state.focused_thought_id() else {
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
        if let Some(thought_id) = self.state.focused_thought_id() {
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
        self.try_reduce_described(action, |cause| cause)
    }

    /// Reduce, reporting a rejection as `describe(cause)` with the exact cause.
    fn try_reduce_described(
        &mut self,
        action: Action,
        describe: impl FnOnce(String) -> String,
    ) -> Option<Vec<Effect>> {
        let may_change_attachments = Self::may_change_attachments(&action);
        match reduce(&mut self.state, action) {
            Ok(effects) => {
                self.finish_successful_reduce(may_change_attachments);
                Some(effects)
            }
            Err(error) => {
                let message = describe(error.to_string());
                if matches!(self.state.durability, DurabilityState::Failed { .. }) {
                    self.set_storage_failure(message);
                } else {
                    self.set_error(message);
                }
                None
            }
        }
    }

    fn finish_successful_reduce(&mut self, may_change_attachments: bool) {
        self.finish_attachment_mutation(may_change_attachments);
        let order = self.live_item_ids();
        self.selection.reconcile(&order);
        self.reconcile_thought_rename();
    }

    pub(super) fn reduce_with_empty_transition(
        &mut self,
        action: Action,
        transition: EmptyBoardTransition,
    ) -> Vec<Effect> {
        self.reduce_with_empty_transition_described(action, transition, |cause| cause)
            .unwrap_or_default()
    }

    /// Like [`Self::reduce_with_empty_transition`], but distinguishes a rejection
    /// (`None`, reported as `describe(cause)`) from success.
    pub(super) fn reduce_with_empty_transition_described(
        &mut self,
        action: Action,
        transition: EmptyBoardTransition,
        describe: impl FnOnce(String) -> String,
    ) -> Option<Vec<Effect>> {
        let was_nonempty = !self.state.board.live_items().is_empty();
        let effects = self.try_reduce_described(action, describe)?;
        if was_nonempty && self.state.board.live_items().is_empty() {
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
        Some(effects)
    }
}
