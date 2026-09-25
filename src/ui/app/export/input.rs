//! Keyboard and pointer routing for the export destination field and confirmation.

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
    ui::{ListNavigation, PointerKind},
};

use super::super::{BoardApp, UiInput, UiKey, query::QueryEditor};
use super::ExportStage;

impl BoardApp {
    pub(in crate::ui::app) fn handle_export_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(state) = self.export.active.as_ref() else {
            return Vec::new();
        };
        match state.stage {
            ExportStage::Path => self.handle_export_path_input(input, ids, clock),
            ExportStage::Confirm { .. } => self.handle_export_confirm_input(input, ids, clock),
            ExportStage::Writing { .. } => Vec::new(),
        }
    }

    fn handle_export_path_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.cancel_export(),
            UiInput::Key(UiKey::Enter) => return self.commit_export_path(ids),
            UiInput::Key(UiKey::Tab) => return self.complete_export_field(false),
            UiInput::Key(UiKey::BackTab) => return self.complete_export_field(true),
            UiInput::Key(UiKey::Backspace) => self.update_export_field(QueryEditor::backspace),
            UiInput::Key(UiKey::Delete | UiKey::ModifiedDelete) => {
                self.update_export_field(QueryEditor::delete);
            }
            UiInput::Key(UiKey::Character(character)) if !character.is_control() => {
                self.update_export_field(|value| value.insert_char(*character));
            }
            UiInput::Key(UiKey::UnmodifiedSpace) => {
                self.update_export_field(|value| value.insert_char(' '));
            }
            UiInput::Key(UiKey::Move {
                movement,
                extend_selection,
            }) => self.update_export_field(|value| {
                value.move_cursor_with_selection(*movement, *extend_selection);
            }),
            UiInput::Key(UiKey::SelectAll) => self.update_export_field(QueryEditor::select_all),
            UiInput::Key(UiKey::Undo) => self.update_export_field(|value| {
                value.undo();
            }),
            UiInput::Key(UiKey::Redo) => self.update_export_field(|value| {
                value.redo();
            }),
            UiInput::Paste(text) => self.update_export_field(|value| value.paste(text)),
            UiInput::PasteAnnotated(payload) => {
                self.update_export_field(|value| value.paste(&payload.content));
            }
            UiInput::Pointer(pointer)
                if !matches!(
                    pointer.kind,
                    PointerKind::ScrollUp | PointerKind::ScrollDown
                ) =>
            {
                return self.handle_pointer(*pointer, ids, clock);
            }
            UiInput::Pointer(_)
            | UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::KeyStroke(_)
            | UiInput::Key(_) => {}
        }
        Vec::new()
    }

    fn handle_export_confirm_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.return_to_export_path(),
            UiInput::Key(UiKey::Enter) => return self.choose_export_replacement(ids),
            UiInput::Key(key) if key.list_navigation().is_some() => {
                if let Some(navigation) = key.list_navigation() {
                    self.move_export_choice(navigation);
                }
            }
            UiInput::Pointer(pointer) => match pointer.kind {
                PointerKind::ScrollUp => self.move_export_choice(ListNavigation::Previous),
                PointerKind::ScrollDown => self.move_export_choice(ListNavigation::Next),
                _ => return self.handle_pointer(*pointer, ids, clock),
            },
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::KeyStroke(_)
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_)
            | UiInput::Key(_) => {}
        }
        Vec::new()
    }

    /// Activate one visible overlay row: the save row, a completion, or a confirmation choice.
    pub(in crate::ui::app) fn activate_export_row(
        &mut self,
        index: usize,
        ids: &mut impl IdGenerator,
    ) -> Vec<Effect> {
        let Some(state) = self.export.active.as_mut() else {
            return Vec::new();
        };
        match &mut state.stage {
            ExportStage::Path if index == 0 => self.commit_export_path(ids),
            ExportStage::Path => {
                self.apply_export_candidate(index - 1);
                Vec::new()
            }
            ExportStage::Confirm { selected, .. } => {
                *selected = index.min(1);
                self.choose_export_replacement(ids)
            }
            ExportStage::Writing { .. } => Vec::new(),
        }
    }

    fn return_to_export_path(&mut self) {
        if let Some(state) = self.export.active.as_mut() {
            state.stage = ExportStage::Path;
        }
        self.set_info("existing file kept; choose another name");
    }

    fn update_export_field(&mut self, update: impl FnOnce(&mut QueryEditor)) {
        if let Some(state) = self.export.active.as_mut()
            && matches!(state.stage, ExportStage::Path)
        {
            let before = state.field.text().to_owned();
            update(&mut state.field);
            if state.field.text() != before {
                state.forget_completion();
            }
        }
    }
}
