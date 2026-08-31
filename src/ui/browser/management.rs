//! Session rename and recoverable trash intentions.

use unicode_segmentation::UnicodeSegmentation;

use crate::ui::{UiInput, UiKey};

use super::{BrowserAction, BrowserAvailability, SessionBrowser};

pub(super) struct RenameState {
    pub(super) session_id: crate::domain::SessionId,
    pub(super) value: String,
}

impl SessionBrowser {
    pub(super) fn begin_rename(&mut self) -> BrowserAction {
        let Some((_, item)) = self.selected_item() else {
            self.status = Some("No matching session".to_owned());
            return BrowserAction::Continue;
        };
        self.rename = Some(RenameState {
            session_id: item.hit.id,
            value: item.hit.name.clone().unwrap_or_default(),
        });
        BrowserAction::Continue
    }

    pub(super) fn trash_selected(&mut self) -> BrowserAction {
        let Some((_, item)) = self.selected_item() else {
            self.status = Some("No matching session".to_owned());
            return BrowserAction::Continue;
        };
        if matches!(item.availability, BrowserAvailability::Trashed) {
            self.status = Some("Session is already in recoverable trash".to_owned());
            return BrowserAction::Continue;
        }
        BrowserAction::Trash(item.hit.id)
    }

    pub(super) fn handle_rename(&mut self, input: UiInput) -> BrowserAction {
        match input {
            UiInput::Key(UiKey::Escape | UiKey::Quit) => {
                self.rename = None;
                BrowserAction::Continue
            }
            UiInput::Key(UiKey::Enter) => {
                let Some(rename) = self.rename.take() else {
                    return BrowserAction::Continue;
                };
                let value = rename.value.trim().to_owned();
                BrowserAction::Rename {
                    session_id: rename.session_id,
                    name: (!value.is_empty()).then_some(value),
                }
            }
            UiInput::Key(UiKey::Backspace | UiKey::Delete | UiKey::ModifiedDelete) => {
                if let Some(rename) = &mut self.rename
                    && let Some((index, _)) = rename.value.grapheme_indices(true).next_back()
                {
                    rename.value.truncate(index);
                }
                BrowserAction::Continue
            }
            UiInput::Key(UiKey::Character(character)) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.push(character);
                }
                BrowserAction::Continue
            }
            UiInput::Paste(text) => {
                if let Some(rename) = &mut self.rename {
                    rename.value.push_str(&text.replace(['\r', '\n'], " "));
                }
                BrowserAction::Continue
            }
            UiInput::PasteAnnotated(payload) => {
                if let Some(rename) = &mut self.rename {
                    rename
                        .value
                        .push_str(&payload.content.replace(['\r', '\n'], " "));
                }
                BrowserAction::Continue
            }
            UiInput::Key(_)
            | UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::Pointer(_) => BrowserAction::Continue,
        }
    }
}
