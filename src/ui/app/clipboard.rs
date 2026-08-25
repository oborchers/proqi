//! Non-destructive clipboard intentions and asynchronous completion.

use crate::{
    application::{Action, ClipboardIntent, Effect, FailureCode, InteractionMode},
    ports::{
        editor::EditCommand,
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, PendingEditorClipboard};

impl BoardApp {
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
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        self.reduce(Action::CopyThought {
            request_id: ids.request_id(),
            thought_id,
        })
    }

    pub(super) fn cut_thought(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        self.reduce(Action::CutThought {
            request_id: ids.request_id(),
            operation_id: ids.operation_id(),
            thought_id,
            at: clock.now(),
        })
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
        self.reduce(Action::ClipboardResult { request_id, result })
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
        let Some((thought_id, editor)) = &self.editor else {
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
            thought_id: *thought_id,
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
