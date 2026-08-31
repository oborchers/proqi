//! Non-destructive clipboard intentions and asynchronous completion.

use crate::{
    application::{Action, ClipboardIntent, Effect, FailureCode, InteractionMode},
    ports::{
        editor::EditCommand,
        environment::{Clock, IdGenerator},
    },
};

use super::{
    BoardApp, ComposePresentation, InsertionConfirmation, InsertionFocus,
    pending_types::PendingEditorClipboard,
};
use crate::ui::PastePayload;

impl BoardApp {
    pub(super) fn paste_payload(
        &mut self,
        payload: PastePayload,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Board) {
            if payload.content.is_empty() {
                return Vec::new();
            }
            self.create(payload, ids, clock)
        } else if matches!(self.state.mode, InteractionMode::Compose) {
            let (content, annotations, verified_paths) = payload.into_parts();
            if content.is_empty() {
                return Vec::new();
            }
            let effects = self.apply_compose_paste(content, &annotations, ids, clock);
            if let InteractionMode::Edit { thought_id } = self.state.mode {
                self.state
                    .attachments
                    .mark_paths_accessible(thought_id, &verified_paths);
            }
            effects
        } else {
            let thought_id = self.active_thought_id();
            let (content, annotations, verified_paths) = payload.into_parts();
            let mut effects = self.flush_pending_edit(ids, clock);
            self.apply_annotated_edit(EditCommand::Paste(content), &annotations);
            effects.extend(self.flush_pending_edit(ids, clock));
            if let Some(thought_id) = thought_id {
                self.state
                    .attachments
                    .mark_paths_accessible(thought_id, &verified_paths);
            }
            effects
        }
    }

    pub(super) fn create(
        &mut self,
        payload: PastePayload,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.clear_board_selection();
        self.compose_presentation = ComposePresentation::Prompt;
        self.insertion_focus = InsertionFocus::Inactive;
        self.insertion_confirmation = InsertionConfirmation::Idle;
        let thought_id = ids.thought_id();
        let (content, annotations, verified_paths) = payload.into_parts();
        let effects = self.reduce(Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content,
            annotations,
            insertion_index: None,
            at: clock.now(),
        });
        self.state
            .attachments
            .mark_paths_accessible(thought_id, &verified_paths);
        self.sync_editor_from_state();
        effects
    }

    pub(super) fn copy_active(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Edit { .. }) {
            self.copy_selection(ids)
        } else {
            self.copy_thought(ids)
        }
    }

    pub(super) fn cut_active(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Edit { .. }) {
            self.cut_selection(ids)
        } else {
            self.cut_thought(ids, clock)
        }
    }

    pub(super) fn copy_thought(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        self.write_board_selection(
            ids,
            ClipboardIntent::Copy,
            crate::domain::Timestamp::default(),
        )
    }

    fn write_board_selection(
        &mut self,
        ids: &mut impl IdGenerator,
        intent: ClipboardIntent,
        at: crate::domain::Timestamp,
    ) -> Vec<Effect> {
        let thought_ids = self.action_thought_ids();
        if thought_ids.is_empty() {
            return Vec::new();
        }
        let request_id = ids.request_id();
        match intent {
            ClipboardIntent::Copy => self.reduce(Action::CopyThoughts {
                request_id,
                thought_ids,
            }),
            ClipboardIntent::Cut => self.reduce(Action::CutThoughts {
                request_id,
                operation_id: ids.operation_id(),
                thought_ids,
                at,
            }),
            ClipboardIntent::CopySessionId | ClipboardIntent::CopyResumeCommand => Vec::new(),
        }
    }

    pub(super) fn copy_session_id(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        self.write_session_metadata(
            ids,
            ClipboardIntent::CopySessionId,
            self.state.board.session.id.to_string(),
        )
    }

    pub(super) fn copy_resume_command(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        self.write_session_metadata(
            ids,
            ClipboardIntent::CopyResumeCommand,
            format!("proqi -r {}", self.state.board.session.id),
        )
    }

    fn write_session_metadata(
        &mut self,
        ids: &mut impl IdGenerator,
        intent: ClipboardIntent,
        content: String,
    ) -> Vec<Effect> {
        let request_id = ids.request_id();
        self.pending_session_clipboard.insert(request_id, intent);
        vec![Effect::WriteClipboard {
            request_id,
            thought_id: None,
            intent,
            content,
        }]
    }

    pub(super) fn cut_thought(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.write_board_selection(ids, ClipboardIntent::Cut, clock.now())
    }

    pub(super) fn copy_selection(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        self.write_selection(ids, ClipboardIntent::Copy)
    }

    pub(super) fn cut_selection(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        self.write_selection(ids, ClipboardIntent::Cut)
    }

    pub(super) fn read_clipboard(&mut self, ids: &mut impl IdGenerator) -> Vec<Effect> {
        let request_id = ids.request_id();
        self.pending_clipboard_reads.insert(request_id);
        vec![Effect::ReadClipboard { request_id }]
    }

    /// Complete one external clipboard write on the reducer-owning UI lane.
    pub fn complete_clipboard_write(
        &mut self,
        request_id: crate::domain::RequestId,
        result: Result<(), FailureCode>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(pending) = self.pending_editor_clipboard.remove(&request_id) {
            return self.complete_editor_clipboard(&pending, result, ids, clock);
        }
        if let Some(intent) = self.pending_session_clipboard.remove(&request_id) {
            match result {
                Ok(()) => match intent {
                    ClipboardIntent::CopySessionId => self.set_success("copied session ID"),
                    ClipboardIntent::CopyResumeCommand => {
                        self.set_success("copied resume command");
                    }
                    ClipboardIntent::Copy | ClipboardIntent::Cut => {}
                },
                Err(code) => self.notify(code),
            }
            return Vec::new();
        }
        let intent = self.state.pending_clipboard_intent(request_id);
        let success = result.is_ok();
        let effects = self.reduce_with_empty_transition(
            Action::ClipboardResult { request_id, result },
            crate::application::EmptyBoardTransition::ComposeAfterLocalRemoval,
        );
        if success && intent == Some(ClipboardIntent::Copy) {
            self.set_success("copied selected thoughts");
        }
        if success && intent == Some(ClipboardIntent::Cut) && !effects.is_empty() {
            self.clear_board_selection();
            self.sync_empty_insertion_focus();
        }
        effects
    }

    /// Complete one plain native clipboard read.
    pub fn complete_clipboard_read(
        &mut self,
        request_id: crate::domain::RequestId,
        result: Result<String, FailureCode>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.complete_clipboard_read_payload(
            request_id,
            result.map(crate::ui::PastePayload::text),
            ids,
            clock,
        )
    }

    /// Complete one native clipboard read without creating an empty thought on failure.
    pub fn complete_clipboard_read_payload(
        &mut self,
        request_id: crate::domain::RequestId,
        result: Result<crate::ui::PastePayload, FailureCode>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if !self.pending_clipboard_reads.remove(&request_id) {
            return Vec::new();
        }
        match result {
            Ok(payload) if payload.content.is_empty() => {
                self.set_warning("clipboard is empty");
                Vec::new()
            }
            Ok(payload) => self.paste_payload(payload, ids, clock),
            Err(code) => {
                self.notify(code);
                Vec::new()
            }
        }
    }

    /// Present an application notification returned by a background effect.
    pub fn notify(&mut self, code: FailureCode) {
        let message = match code {
            FailureCode::ClipboardFailed => {
                "clipboard unavailable; use bracketed terminal paste or retry".to_owned()
            }
            FailureCode::StorageFailed => {
                "save failed; press r to retry or w to export recovery".to_owned()
            }
            FailureCode::RecoveryCapacity => "save failed; press w to export recovery".to_owned(),
            _ => code.as_str().to_owned(),
        };
        self.set_error(message);
    }

    fn write_selection(
        &mut self,
        ids: &mut impl IdGenerator,
        intent: ClipboardIntent,
    ) -> Vec<Effect> {
        let Some((super::EditorOwner::Thought(thought_id), editor)) = &self.editor else {
            return Vec::new();
        };
        let Some(content) = editor.selected_text() else {
            self.set_warning("select text before copying or cutting");
            return Vec::new();
        };
        let request_id = ids.request_id();
        self.pending_editor_clipboard.insert(
            request_id,
            PendingEditorClipboard {
                intent,
                before: editor.snapshot(),
            },
        );
        vec![Effect::WriteClipboard {
            request_id,
            thought_id: Some(*thought_id),
            intent,
            content,
        }]
    }

    fn complete_editor_clipboard(
        &mut self,
        pending: &PendingEditorClipboard,
        result: Result<(), FailureCode>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Err(code) = result {
            self.notify(code);
            return Vec::new();
        }
        if pending.intent == ClipboardIntent::Copy {
            self.set_success("copied selection");
            return Vec::new();
        }
        if matches!(
            self.state.durability,
            crate::application::DurabilityState::Failed { .. }
        ) {
            self.set_error("storage failed, selection was copied without deletion");
            return Vec::new();
        }
        let unchanged = self.editor_snapshot().is_some_and(|current| {
            current.content == pending.before.content
                && current.cursor == pending.before.cursor
                && current.selection == pending.before.selection
        });
        if !unchanged || !matches!(self.state.mode, InteractionMode::Edit { .. }) {
            self.set_warning("selection changed before clipboard confirmation");
            return Vec::new();
        }
        self.apply_edit(EditCommand::DeleteForward);
        self.flush_pending_edit(ids, clock)
    }
}
