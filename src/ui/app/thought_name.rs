//! Inline editing for optional thought names.

use crate::{
    application::{Action, Effect},
    domain::{THOUGHT_NAME_MAX_CHARS, ThoughtId, ThoughtName},
    ports::environment::{Clock, IdGenerator},
};

use super::{
    BoardApp, HitTarget, PointerButton, PointerInput, PointerKind, UiInput, UiKey,
    pending_types::EditFlush, query::QueryEditor,
};

pub(super) struct ThoughtNameState {
    pub(super) thought_id: ThoughtId,
    pub(super) editor: QueryEditor,
}

impl BoardApp {
    pub(super) fn begin_thought_rename(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought_id) = self.active_thought_id() else {
            self.set_warning("select a thought before naming it");
            return Vec::new();
        };
        self.begin_thought_rename_for(thought_id, ids, clock)
    }

    pub(super) fn begin_thought_rename_for(
        &mut self,
        thought_id: ThoughtId,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.submission_locked(thought_id) {
            self.set_warning("thought has a submission in progress");
            return Vec::new();
        }
        if self.state.deferred_board_operation_pending() {
            self.set_warning("wait for the pending board save before renaming");
            return Vec::new();
        }
        let effects = match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        let Some(name) = self
            .state
            .board
            .thought(thought_id)
            .map(|thought| thought.name.clone())
        else {
            return effects;
        };
        self.deactivate_range_latch();
        self.thought_rename = Some(ThoughtNameState {
            thought_id,
            editor: QueryEditor::from_text_with_character_limit(
                name.as_ref().map_or("", ThoughtName::as_str),
                THOUGHT_NAME_MAX_CHARS,
            ),
        });
        self.layout = None;
        self.frame_presentation = None;
        effects
    }

    pub(super) fn begin_thought_rename_from_pointer(
        &mut self,
        thought_id: ThoughtId,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let initial_position = self
            .layout
            .as_ref()
            .and_then(|layout| layout.thought(thought_id))
            .and_then(|thought| thought.name)
            .and_then(|area| {
                self.state
                    .board
                    .thought(thought_id)
                    .and_then(|thought| thought.name.as_ref())
                    .map(|name| {
                        let visible_width = usize::from(area.width).saturating_sub(1);
                        let relative =
                            usize::from(pointer.column.saturating_sub(area.x)).min(visible_width);
                        crate::ports::text_layout::byte_at_display_cell(name.as_str(), relative)
                    })
            });
        let effects = self.begin_thought_rename_for(thought_id, ids, clock);
        if let Some(byte) = initial_position {
            self.update_thought_name(|editor| editor.place_cursor(byte, pointer.extend_selection));
        }
        effects
    }

    pub(super) fn handle_thought_rename(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.cancel_thought_rename(),
            UiInput::Key(UiKey::Enter) => return self.commit_thought_rename(ids, clock),
            UiInput::Key(UiKey::Backspace) => self.update_thought_name(QueryEditor::backspace),
            UiInput::Key(UiKey::Delete | UiKey::ModifiedDelete) => {
                self.update_thought_name(QueryEditor::delete);
            }
            UiInput::Key(UiKey::Character(character)) if !character.is_control() => {
                self.update_thought_name(|value| value.insert_char(*character));
            }
            UiInput::Key(UiKey::UnmodifiedSpace) => {
                self.update_thought_name(|value| value.insert_char(' '));
            }
            UiInput::Key(UiKey::Move {
                movement,
                extend_selection,
            }) => self.update_thought_name(|value| {
                value.move_cursor_with_selection(*movement, *extend_selection);
            }),
            UiInput::Key(UiKey::SelectAll) => self.update_thought_name(QueryEditor::select_all),
            UiInput::Key(UiKey::Undo) => self.update_thought_name(|value| {
                value.undo();
            }),
            UiInput::Key(UiKey::Redo) => self.update_thought_name(|value| {
                value.redo();
            }),
            UiInput::Paste(text) => self.update_thought_name(|value| value.paste(text)),
            UiInput::PasteAnnotated(payload) => {
                self.update_thought_name(|value| value.paste(&payload.content));
            }
            UiInput::Pointer(pointer) => {
                return self.handle_thought_name_pointer(*pointer, ids, clock);
            }
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::KeyStroke(_)
            | UiInput::Key(_) => {}
        }
        Vec::new()
    }

    fn handle_thought_name_pointer(
        &mut self,
        pointer: PointerInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let hit = self.hit(pointer);
        if matches!(pointer.kind, PointerKind::Move) {
            return Vec::new();
        }
        let left_down = matches!(pointer.kind, PointerKind::Down(PointerButton::Left));
        if left_down && hit == Some(HitTarget::CommitThoughtName) {
            return self.commit_thought_rename(ids, clock);
        }
        if left_down && hit == Some(HitTarget::CancelThoughtName) {
            self.cancel_thought_rename();
            return Vec::new();
        }
        let active_name = self
            .thought_rename
            .as_ref()
            .map(|state| HitTarget::ThoughtName(state.thought_id));
        if hit == active_name
            && matches!(
                pointer.kind,
                PointerKind::Down(PointerButton::Left) | PointerKind::Drag(PointerButton::Left)
            )
        {
            self.place_thought_name_cursor(pointer);
            return Vec::new();
        }
        if left_down {
            return self.commit_thought_rename(ids, clock);
        }
        Vec::new()
    }

    fn place_thought_name_cursor(&mut self, pointer: PointerInput) {
        let Some(state) = &self.thought_rename else {
            return;
        };
        let Some(area) = self
            .layout
            .as_ref()
            .and_then(|layout| layout.thought(state.thought_id))
            .and_then(|thought| thought.name)
        else {
            return;
        };
        self.place_thought_name_cursor_in_area(pointer, area);
    }

    fn place_thought_name_cursor_in_area(
        &mut self,
        pointer: PointerInput,
        area: ratatui_core::layout::Rect,
    ) {
        let Some(state) = &self.thought_rename else {
            return;
        };
        let width = usize::from(area.width).saturating_sub(1);
        let window = crate::ports::text_layout::visible_cell_window(
            state.editor.text(),
            state.editor.cursor(),
            width,
        );
        let relative = usize::from(pointer.column.saturating_sub(area.x)).min(width);
        let byte = crate::ports::text_layout::byte_at_display_cell(
            state.editor.text(),
            window.start_cell.saturating_add(relative),
        );
        let extend = matches!(pointer.kind, PointerKind::Drag(PointerButton::Left))
            || pointer.extend_selection;
        self.update_thought_name(|editor| editor.place_cursor(byte, extend));
    }

    fn update_thought_name(&mut self, update: impl FnOnce(&mut QueryEditor)) {
        if let Some(state) = &mut self.thought_rename {
            update(&mut state.editor);
            self.frame_presentation = None;
        }
    }

    fn cancel_thought_rename(&mut self) {
        self.thought_rename = None;
        self.layout = None;
        self.frame_presentation = None;
    }

    fn commit_thought_rename(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(state) = self.thought_rename.take() else {
            return Vec::new();
        };
        let trimmed = state.editor.text().trim();
        let name = if trimmed.is_empty() {
            None
        } else {
            match ThoughtName::new(trimmed.to_owned()) {
                Ok(name) => Some(name),
                Err(error) => {
                    self.set_error(error.to_string());
                    self.thought_rename = Some(state);
                    return Vec::new();
                }
            }
        };
        if self
            .state
            .board
            .thought(state.thought_id)
            .is_some_and(|thought| thought.name == name)
        {
            self.layout = None;
            self.frame_presentation = None;
            return Vec::new();
        }
        let effects = self.reduce(Action::RenameThought {
            operation_id: ids.operation_id(),
            thought_id: state.thought_id,
            name,
            at: clock.now(),
        });
        if effects.is_empty() {
            self.thought_rename = Some(state);
        } else {
            self.layout = None;
            self.frame_presentation = None;
        }
        effects
    }

    pub(super) fn reconcile_thought_rename(&mut self) {
        let missing = self.thought_rename.as_ref().is_some_and(|rename| {
            !self
                .state
                .board
                .thought(rename.thought_id)
                .is_some_and(crate::domain::Thought::is_live)
        });
        if missing {
            self.cancel_thought_rename();
        }
    }

    pub(in crate::ui) fn thought_name_editor(&self, thought_id: ThoughtId) -> Option<&QueryEditor> {
        self.thought_rename
            .as_ref()
            .filter(|state| state.thought_id == thought_id)
            .map(|state| &state.editor)
    }

    pub(in crate::ui) const fn thought_name_editing(&self) -> bool {
        self.thought_rename.is_some()
    }
}
