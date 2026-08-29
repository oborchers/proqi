//! Semantic editor coalescing before durable reducer revisions.

use crate::{
    application::{Action, Effect, InteractionMode, reduce},
    domain::{ContentAnnotation, ThoughtId},
    ports::{
        editor::{CursorMovement, EditCommand, EditorSnapshot},
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, UiInput, UiKey};
use crate::ui::annotations;

pub(super) fn command_for_key(key: UiKey, adjacent_fold: bool) -> Option<(EditCommand, bool)> {
    match key {
        UiKey::Character(character) => Some((EditCommand::InsertChar(character), false)),
        UiKey::Enter => Some((EditCommand::InsertNewline, false)),
        UiKey::Backspace => Some((EditCommand::DeleteBack, adjacent_fold)),
        UiKey::Delete => Some((EditCommand::DeleteForward, adjacent_fold)),
        UiKey::Move {
            movement,
            extend_selection,
        } => Some((
            EditCommand::Move {
                movement,
                extend_selection,
            },
            true,
        )),
        UiKey::SelectAll => Some((EditCommand::SelectAll, true)),
        UiKey::DeleteLine => Some((EditCommand::DeleteLogicalLine, true)),
        UiKey::Escape
        | UiKey::EditNavigation { .. }
        | UiKey::PrimaryCharacter(_)
        | UiKey::PrimaryShiftMove { .. }
        | UiKey::Undo
        | UiKey::Redo
        | UiKey::Quit
        | UiKey::Copy
        | UiKey::Cut
        | UiKey::PasteClipboard
        | UiKey::Duplicate
        | UiKey::Tab
        | UiKey::BackTab
        | UiKey::PickerPrevious
        | UiKey::PickerNext => None,
    }
}

pub(super) struct PendingEdit {
    thought_id: ThoughtId,
    before: EditorSnapshot,
    after: EditorSnapshot,
    before_annotations: Vec<ContentAnnotation>,
    after_annotations: Vec<ContentAnnotation>,
}

impl BoardApp {
    pub(super) fn resolve_edit_navigation(&self, input: UiInput) -> UiInput {
        let UiInput::Key(UiKey::EditNavigation {
            editor_movement,
            board_movement,
        }) = input
        else {
            return input;
        };
        let overlay_open = self.help
            || self.update_prompt.is_some()
            || self.palette.is_some()
            || self.invocation_popup.is_some()
            || self.transfer.is_some()
            || self.rename.is_some()
            || self.search.is_some()
            || self.submission_mode.is_some();
        let movement =
            if !overlay_open && matches!(self.interaction_mode(), InteractionMode::Edit { .. }) {
                editor_movement
            } else {
                board_movement
            };
        UiInput::Key(UiKey::Move {
            movement,
            extend_selection: false,
        })
    }

    pub(super) fn should_insert_smart_newline(&self) -> bool {
        self.settings.smart_lists
            && self
                .editor_snapshot()
                .is_some_and(|snapshot| snapshot.selection.is_none())
    }

    pub(super) fn insert_newline(
        &mut self,
        smart: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let mut effects = self.flush_pending_edit(ids, clock);
        self.apply_edit(if smart {
            EditCommand::InsertSmartNewline {
                indent_width: self.settings.list_indent_width,
            }
        } else {
            EditCommand::InsertNewline
        });
        effects.extend(self.flush_pending_edit(ids, clock));
        effects
    }

    pub(super) fn apply_indentation(
        &mut self,
        outdent: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let mut effects = self.flush_pending_edit(ids, clock);
        let command = if outdent {
            EditCommand::Outdent {
                width: self.settings.list_indent_width,
                smart_lists: self.settings.smart_lists,
            }
        } else {
            EditCommand::Indent {
                width: self.settings.list_indent_width,
                smart_lists: self.settings.smart_lists,
            }
        };
        self.apply_edit(command);
        effects.extend(self.flush_pending_edit(ids, clock));
        effects
    }

    pub(super) fn finish_boundary_navigation(
        &mut self,
        movement: CursorMovement,
        extend_selection: bool,
        before: Option<&EditorSnapshot>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if extend_selection
            || !matches!(
                movement,
                CursorMovement::VisualUp | CursorMovement::VisualDown
            )
        {
            self.edit_boundary = None;
            return Vec::new();
        }
        let Some(before) = before else {
            return Vec::new();
        };
        let Some(after) = self.editor_snapshot() else {
            return Vec::new();
        };
        if before.cursor != after.cursor || before.selection != after.selection {
            self.edit_boundary = None;
            return Vec::new();
        }
        let armed = self.edit_boundary == Some(movement);
        self.edit_boundary = Some(movement);
        if !armed {
            return Vec::new();
        }
        let target = self.edit_neighbor(movement);
        if target.is_none() && movement != CursorMovement::VisualDown {
            return Vec::new();
        }
        if target.is_none() && after.content.is_empty() {
            self.edit_boundary = None;
            return Vec::new();
        }
        let mut effects = self.finish_edit(ids, clock);
        self.palette_selection_handoff = None;
        if let Some(target) = target {
            self.insertion_focus = super::InsertionFocus::Inactive;
            effects.extend(self.reduce(Action::FocusThought(Some(target))));
        } else {
            effects.extend(self.create(crate::ui::PastePayload::text(String::new()), ids, clock));
        }
        effects
    }

    fn edit_neighbor(&self, movement: CursorMovement) -> Option<ThoughtId> {
        let live = self.state.board.live_thoughts();
        let active = self.active_thought_id()?;
        let current = live.iter().position(|thought| thought.id == active)?;
        let target = match movement {
            CursorMovement::VisualUp => current.checked_sub(1)?,
            CursorMovement::VisualDown => current.saturating_add(1),
            _ => return None,
        };
        live.get(target).map(|thought| thought.id)
    }

    pub(super) fn current_annotations(&self, thought_id: ThoughtId) -> Vec<ContentAnnotation> {
        self.pending_edit
            .as_ref()
            .filter(|pending| pending.thought_id == thought_id)
            .map(|pending| pending.after_annotations.clone())
            .or_else(|| {
                self.state
                    .board
                    .thought(thought_id)
                    .map(|thought| thought.annotations.clone())
            })
            .unwrap_or_default()
    }

    pub(super) fn apply_edit(&mut self, command: EditCommand) {
        self.apply_annotated_edit(command, &[]);
    }

    pub(super) fn apply_annotated_edit(
        &mut self,
        command: EditCommand,
        inserted_annotations: &[ContentAnnotation],
    ) {
        let edit = self.editor.as_mut().and_then(|(thought_id, editor)| {
            let before = editor.snapshot();
            let outcome = editor.apply(command);
            (!outcome.changes.is_empty()).then_some((
                *thought_id,
                before,
                outcome.snapshot,
                outcome.changes,
            ))
        });
        let Some((thought_id, before, after, changes)) = edit else {
            return;
        };
        self.clear_expanded_folds(thought_id);
        let current_annotations = self
            .pending_edit
            .as_ref()
            .filter(|pending| pending.thought_id == thought_id)
            .map_or_else(
                || {
                    self.state
                        .board
                        .thought(thought_id)
                        .map_or_else(Vec::new, |thought| thought.annotations.clone())
                },
                |pending| pending.after_annotations.clone(),
            );
        let after_annotations = annotations::rebase(
            &before.content,
            &after.content,
            &changes,
            &current_annotations,
            inserted_annotations,
        );
        match &mut self.pending_edit {
            Some(pending) if pending.thought_id == thought_id => {
                pending.after = after;
                pending.after_annotations = after_annotations;
            }
            _ => {
                self.pending_edit = Some(PendingEdit {
                    thought_id,
                    before,
                    after,
                    before_annotations: current_annotations,
                    after_annotations,
                });
            }
        }
        self.edit_generation = self.edit_generation.wrapping_add(1);
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        self.layout = None;
    }

    /// Commit accumulated typing as one persistent editor revision.
    pub fn flush_pending_edit(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(pending) = self.pending_edit.as_ref() else {
            return Vec::new();
        };
        let action = Action::EditThought {
            thought_id: pending.thought_id,
            revision_id: ids.revision_id(),
            before_content: pending.before.content.clone(),
            after_content: pending.after.content.clone(),
            before_annotations: pending.before_annotations.clone(),
            after_annotations: pending.after_annotations.clone(),
            before_cursor: pending.before.cursor,
            after_cursor: pending.after.cursor,
            at: clock.now(),
        };
        match reduce(&mut self.state, action) {
            Ok(effects) => {
                self.pending_edit = None;
                effects
            }
            Err(error) => {
                self.set_error(error.to_string());
                Vec::new()
            }
        }
    }

    pub(super) fn pending_edit_snapshot(&self) -> Option<(ThoughtId, &EditorSnapshot)> {
        self.pending_edit
            .as_ref()
            .map(|pending| (pending.thought_id, &pending.after))
    }
}
