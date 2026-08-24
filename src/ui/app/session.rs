//! Current-session naming workflow.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::{
    application::{Action, Effect},
    ports::store::StoreError,
    ui::HitTarget,
};

use super::{BoardApp, PointerButton, PointerKind, UiInput, UiKey};

impl BoardApp {
    pub(super) fn begin_session_rename(&mut self) {
        self.help = false;
        self.palette = None;
        self.search = None;
        self.rename = Some(self.state.board.session.name.clone().unwrap_or_default());
    }

    pub(super) fn handle_session_rename(&mut self, input: &UiInput) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.rename = None,
            UiInput::Key(UiKey::Enter) => return self.commit_session_rename(),
            UiInput::Key(UiKey::Backspace) => {
                if let Some(value) = &mut self.rename
                    && let Some((index, _)) = value.grapheme_indices(true).next_back()
                {
                    value.truncate(index);
                }
            }
            UiInput::Key(UiKey::Character(character)) if !character.is_control() => {
                if let Some(value) = &mut self.rename {
                    value.push(*character);
                }
            }
            UiInput::Paste(text) => self.append_session_name(text),
            UiInput::PasteAnnotated(payload) => self.append_session_name(&payload.content),
            UiInput::Pointer(pointer)
                if matches!(pointer.kind, PointerKind::Down(PointerButton::Left))
                    && self.layout.as_ref().is_some_and(|layout| {
                        layout.hit_test(pointer.column, pointer.row)
                            == Some(HitTarget::CloseOverlay)
                    }) =>
            {
                self.rename = None;
            }
            UiInput::Pointer(_)
            | UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::Key(_) => {}
        }
        Vec::new()
    }

    fn append_session_name(&mut self, text: &str) {
        if let Some(value) = &mut self.rename {
            value.push_str(&text.replace(['\r', '\n'], " "));
        }
    }

    fn commit_session_rename(&mut self) -> Vec<Effect> {
        let Some(value) = self.rename.take() else {
            return Vec::new();
        };
        let trimmed = value.trim();
        let name = (!trimmed.is_empty()).then(|| trimmed.to_owned());
        self.reduce(Action::RenameSession { name })
    }

    /// Resolve one asynchronous current-session rename without lying about persistence.
    pub fn complete_session_rename(
        &mut self,
        previous_name: Option<String>,
        result: Result<(), StoreError>,
    ) {
        match result {
            Ok(()) => self.status = Some("session renamed".to_owned()),
            Err(error) => {
                let _restored = self.state.board.session.rename(previous_name);
                self.status = Some(format!("session rename failed: {error}"));
            }
        }
    }
}
