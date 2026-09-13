//! Session rename and recoverable trash intentions.

use crate::ui::input::{RoutedInput as UiInput, UiKey};

use super::{BrowserAction, BrowserAvailability, SessionBrowser};

pub(super) struct RenameState {
    pub(super) session_id: crate::domain::SessionId,
    pub(super) value: crate::ui::app::query::QueryEditor,
}

impl SessionBrowser {
    pub(super) fn begin_rename(&mut self) -> BrowserAction {
        let Some((_, item)) = self.selected_item() else {
            self.status = Some("No matching session".to_owned());
            return BrowserAction::Continue;
        };
        self.rename = Some(RenameState {
            session_id: item.hit.id,
            value: crate::ui::app::query::QueryEditor::from_text(
                item.hit.name.as_deref().unwrap_or_default(),
            ),
        });
        self.layout = None;
        BrowserAction::Continue
    }

    pub(super) fn trash_selected(&mut self) -> BrowserAction {
        let Some((_, item)) = self.selected_item() else {
            self.status = Some("No matching session".to_owned());
            return BrowserAction::Continue;
        };
        if matches!(item.availability, BrowserAvailability::Trashed) {
            return BrowserAction::Restore(item.hit.id);
        }
        BrowserAction::Trash(item.hit.id)
    }

    pub(super) fn handle_rename(&mut self, input: UiInput) -> BrowserAction {
        match input {
            UiInput::Pointer(pointer)
                if pointer.kind == crate::ui::PointerKind::Down(crate::ui::PointerButton::Left) =>
            {
                self.handle_rename_pointer(pointer)
            }
            UiInput::Key(UiKey::Escape | UiKey::Quit) => {
                self.cancel_rename();
                BrowserAction::Continue
            }
            UiInput::Key(UiKey::Enter) => self.confirm_rename(),
            input => {
                self.edit_rename(input);
                BrowserAction::Continue
            }
        }
    }

    fn handle_rename_pointer(&mut self, pointer: crate::ui::PointerInput) -> BrowserAction {
        let hit = self
            .layout
            .as_ref()
            .map_or(super::BrowserHit::None, |layout| {
                layout.hit_test(pointer.column, pointer.row, &self.footer_controls)
            });
        if hit == super::BrowserHit::Confirm {
            return self.confirm_rename();
        }
        if hit == super::BrowserHit::Cancel {
            self.cancel_rename();
        }
        BrowserAction::Continue
    }

    fn edit_rename(&mut self, input: UiInput) {
        match input {
            UiInput::Key(UiKey::Backspace) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.backspace();
                }
            }
            UiInput::Key(UiKey::Delete | UiKey::ModifiedDelete) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.delete();
                }
            }
            UiInput::Key(UiKey::Character(character)) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.insert_char(character);
                }
            }
            UiInput::Key(UiKey::UnmodifiedSpace) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.insert_char(' ');
                }
            }
            UiInput::Paste(text) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.paste(&text);
                }
            }
            UiInput::PasteAnnotated(payload) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.paste(&payload.content);
                }
            }
            UiInput::Key(UiKey::Move {
                movement,
                extend_selection,
            }) => {
                if let Some(rename) = &mut self.rename {
                    rename
                        .value
                        .move_cursor_with_selection(movement, extend_selection);
                }
            }
            UiInput::Key(UiKey::SelectAll) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.select_all();
                }
            }
            UiInput::Key(UiKey::Undo) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.undo();
                }
            }
            UiInput::Key(UiKey::Redo) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.redo();
                }
            }
            UiInput::Key(_)
            | UiInput::KeyStroke(_)
            | UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::Pointer(_) => {}
        }
    }

    fn cancel_rename(&mut self) {
        self.rename = None;
        self.layout = None;
    }

    fn confirm_rename(&mut self) -> BrowserAction {
        let Some(rename) = self.rename.take() else {
            return BrowserAction::Continue;
        };
        self.layout = None;
        let value = rename.value.text().trim().to_owned();
        BrowserAction::Rename {
            session_id: rename.session_id,
            name: (!value.is_empty()).then_some(value),
        }
    }
}
