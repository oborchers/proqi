//! Non-destructive clipboard intentions and asynchronous completion.

mod messages;
use crate::{
    application::{Action, ClipboardIntent, Effect, FailureCode, InteractionMode},
    domain::extract_annotations,
    ports::{
        editor::EditCommand,
        environment::{Clock, IdGenerator},
    },
};

use super::{
    BoardApp, ComposePresentation, EditorOwner, InsertionConfirmation, InsertionFocus,
    pending_types::{ClipboardReadOwner, EditFlush, PendingEditorClipboard},
};
use crate::ui::PastePayload;

impl BoardApp {
    fn apply_preserved_edit(
        &mut self,
        command: EditCommand,
        inserted_annotations: &[crate::domain::ContentAnnotation],
    ) {
        self.apply_annotated_edit_with_policy(command, inserted_annotations, true);
    }

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
            if self.insertion_focused() {
                self.create_at_bottom(payload, ids, clock)
            } else {
                self.create(payload, ids, clock)
            }
        } else if matches!(self.state.mode, InteractionMode::Compose) {
            let (content, annotations, verified_paths, preserve_owned) = payload.into_parts();
            if content.is_empty() {
                return Vec::new();
            }
            let effects =
                self.apply_compose_paste(content, &annotations, preserve_owned, ids, clock);
            if let InteractionMode::Edit { thought_id } = self.state.mode {
                self.state
                    .attachments
                    .mark_paths_accessible(thought_id, &verified_paths);
            }
            effects
        } else {
            let thought_id = self.active_thought_id();
            let (content, annotations, verified_paths, preserve_owned) = payload.into_parts();
            let mut effects = self.flush_pending_edit(ids, clock);
            if preserve_owned {
                self.apply_preserved_edit(EditCommand::Paste(content), &annotations);
            } else {
                self.apply_annotated_edit(EditCommand::Paste(content), &annotations);
            }
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
        self.create_with_insertion_index(payload, None, ids, clock)
    }

    pub(super) fn create_at(
        &mut self,
        payload: PastePayload,
        insertion_index: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.create_with_insertion_index(payload, Some(insertion_index), ids, clock)
    }

    fn create_with_insertion_index(
        &mut self,
        payload: PastePayload,
        insertion_index: Option<usize>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.clear_board_selection();
        self.compose_presentation = ComposePresentation::Prompt;
        self.insertion_focus = InsertionFocus::Inactive;
        self.insertion_confirmation = InsertionConfirmation::Idle;
        let thought_id = ids.thought_id();
        let (content, annotations, verified_paths, preserve_owned) = payload.into_parts();
        let operation_id = ids.operation_id();
        let at = clock.now();
        let action = if preserve_owned {
            Action::CreateOwnedThought(crate::application::OwnedThoughtCreation::preserved(
                thought_id,
                operation_id,
                content,
                annotations,
                insertion_index,
                at,
            ))
        } else {
            Action::CreateThought {
                thought_id,
                operation_id,
                content,
                annotations,
                insertion_index,
                at,
            }
        };
        let effects = self.reduce(action);
        if matches!(
            self.state.mode,
            InteractionMode::Edit {
                thought_id: active
            } if active == thought_id
        ) {
            self.board_viewport = self.board_viewport.follow_focus();
            self.scroll_geometry = None;
            self.layout = None;
        }
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
            annotations: Vec::new(),
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
        let owner = match self.state.mode {
            InteractionMode::Board => ClipboardReadOwner::Board,
            InteractionMode::Compose => ClipboardReadOwner::Compose {
                generation: self.compose_generation,
            },
            InteractionMode::Edit { thought_id } => ClipboardReadOwner::Thought {
                thought_id,
                generation: self.edit_owner_generation,
            },
        };
        self.pending_clipboard_reads.insert(request_id, owner);
        vec![Effect::ReadClipboard { request_id }]
    }

    pub(super) fn rebind_compose_clipboard_reads(
        &mut self,
        generation: u64,
        thought_id: crate::domain::ThoughtId,
    ) {
        for owner in self.pending_clipboard_reads.values_mut() {
            if matches!(
                owner,
                ClipboardReadOwner::Compose {
                    generation: owner_generation
                } if *owner_generation == generation
            ) {
                *owner = ClipboardReadOwner::Thought {
                    thought_id,
                    generation: self.edit_owner_generation,
                };
            }
        }
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
        let (mut effects, completion) = if success && intent == Some(ClipboardIntent::Cut) {
            match self.flush_edit_boundary(ids, clock) {
                EditFlush::Complete(effects) => (effects, result),
                EditFlush::Blocked(effects) => (effects, Err(FailureCode::ContentConflict)),
            }
        } else {
            (Vec::new(), result)
        };
        effects.extend(self.reduce_with_empty_transition(
            Action::ClipboardResult {
                request_id,
                result: completion,
            },
            crate::application::EmptyBoardTransition::ComposeAfterLocalRemoval,
        ));
        if success && intent == Some(ClipboardIntent::Copy) {
            self.set_success("copied selected thoughts");
        }
        let completed_cut = effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::CommitBoardOperation(operation)
                    if operation.kind == crate::domain::BoardOperationKind::Cut
            )
        });
        if success && intent == Some(ClipboardIntent::Cut) && completed_cut {
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
        let Some(owner) = self.pending_clipboard_reads.remove(&request_id) else {
            return Vec::new();
        };
        if !self.clipboard_read_owner_is_current(owner) {
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

    fn clipboard_read_owner_is_current(&self, owner: ClipboardReadOwner) -> bool {
        match owner {
            ClipboardReadOwner::Board => matches!(self.state.mode, InteractionMode::Board),
            ClipboardReadOwner::Compose { generation } => {
                generation == self.compose_generation
                    && matches!(self.state.mode, InteractionMode::Compose)
                    && matches!(self.editor.as_ref(), Some((EditorOwner::Compose, _)))
            }
            ClipboardReadOwner::Thought {
                thought_id: expected,
                generation,
            } => {
                matches!(
                    self.state.mode,
                    InteractionMode::Edit { thought_id } if thought_id == expected
                ) && generation == self.edit_owner_generation
                    && matches!(
                        self.editor.as_ref(),
                        Some((EditorOwner::Thought(actual), _)) if *actual == expected
                    )
                    && !self.edit_content_mutation_blocked(expected)
            }
        }
    }

    fn write_selection(
        &mut self,
        ids: &mut impl IdGenerator,
        intent: ClipboardIntent,
    ) -> Vec<Effect> {
        self.normalize_clipboard_selection();
        let Some((super::EditorOwner::Thought(thought_id), editor)) = &self.editor else {
            return Vec::new();
        };
        let snapshot = editor.snapshot();
        let Some(selection) = snapshot.selection else {
            self.set_warning("select text before copying or cutting");
            return Vec::new();
        };
        let start =
            crate::ports::text_layout::byte_for_position(&snapshot.content, selection.start);
        let end = crate::ports::text_layout::byte_for_position(&snapshot.content, selection.end);
        let current_annotations = self.current_annotations(*thought_id);
        let Ok((_, annotations)) =
            extract_annotations(&snapshot.content, &current_annotations, start..end)
        else {
            self.set_error("selection metadata is invalid");
            return Vec::new();
        };
        let content = snapshot.content[start..end].to_owned();
        let request_id = ids.request_id();
        self.pending_editor_clipboard.insert(
            request_id,
            PendingEditorClipboard {
                intent,
                thought_id: *thought_id,
                edit_owner_generation: self.edit_owner_generation,
                before: snapshot,
                source_annotations: current_annotations,
            },
        );
        vec![Effect::WriteClipboard {
            request_id,
            thought_id: Some(*thought_id),
            intent,
            content,
            annotations,
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
        let owner_is_current = matches!(
            self.state.mode,
            InteractionMode::Edit { thought_id } if thought_id == pending.thought_id
        ) && self.edit_owner_generation == pending.edit_owner_generation
            && matches!(
                self.editor.as_ref(),
                Some((EditorOwner::Thought(thought_id), _)) if *thought_id == pending.thought_id
            );
        let editor_is_unchanged = self.editor_snapshot().is_some_and(|current| {
            current.content == pending.before.content
                && current.cursor == pending.before.cursor
                && current.selection == pending.before.selection
        });
        let annotations_are_unchanged =
            self.current_annotations(pending.thought_id) == pending.source_annotations;
        if !owner_is_current || !editor_is_unchanged || !annotations_are_unchanged {
            self.set_warning("selection changed before clipboard confirmation");
            return Vec::new();
        }
        self.apply_edit(EditCommand::DeleteForward);
        self.flush_pending_edit(ids, clock)
    }
}
