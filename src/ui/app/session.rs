//! Current-session naming workflow.

use crate::{
    application::{Action, Effect},
    ports::environment::{Clock, IdGenerator},
    ports::store::StoreError,
    ui::HitTarget,
};

use super::{
    BoardApp, PointerButton, PointerKind, SessionRenamePersistence, UiInput, UiKey,
    query::QueryEditor,
};

impl BoardApp {
    pub(super) fn begin_session_rename(&mut self) {
        if self.session_rename_persistence.is_saving() {
            self.set_info("session rename is still saving");
            return;
        }
        self.deactivate_range_latch();
        self.help = false;
        self.palette = None;
        self.search = None;
        self.rename = Some(QueryEditor::from_text(
            self.state.board.session.name.as_deref().unwrap_or_default(),
        ));
    }

    pub(super) fn handle_session_rename(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::Key(UiKey::Escape) => self.rename = None,
            UiInput::Key(UiKey::Enter) => return self.commit_session_rename(ids, clock),
            UiInput::Key(UiKey::Backspace) => self.update_session_name(QueryEditor::backspace),
            UiInput::Key(UiKey::Delete | UiKey::ModifiedDelete) => {
                self.update_session_name(QueryEditor::delete);
            }
            UiInput::Key(UiKey::Character(character)) if !character.is_control() => {
                self.update_session_name(|value| value.insert_char(*character));
            }
            UiInput::Key(UiKey::UnmodifiedSpace) => {
                self.update_session_name(|value| value.insert_char(' '));
            }
            UiInput::Key(UiKey::Move {
                movement,
                extend_selection,
            }) => self.update_session_name(|value| {
                value.move_cursor_with_selection(*movement, *extend_selection);
            }),
            UiInput::Key(UiKey::SelectAll) => self.update_session_name(QueryEditor::select_all),
            UiInput::Key(UiKey::Undo) => self.update_session_name(|value| {
                value.undo();
            }),
            UiInput::Key(UiKey::Redo) => self.update_session_name(|value| {
                value.redo();
            }),
            UiInput::Paste(text) => self.update_session_name(|value| value.paste(text)),
            UiInput::PasteAnnotated(payload) => {
                self.update_session_name(|value| value.paste(&payload.content));
            }
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
            | UiInput::HostFocusLost
            | UiInput::KeyStroke(_)
            | UiInput::Key(_) => {}
        }
        Vec::new()
    }

    fn update_session_name(&mut self, update: impl FnOnce(&mut QueryEditor)) {
        if let Some(value) = &mut self.rename {
            update(value);
        }
    }

    fn commit_session_rename(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.session_rename_persistence.is_saving() {
            self.set_info("session rename is still saving");
            return Vec::new();
        }
        let Some(value) = self.rename.take() else {
            return Vec::new();
        };
        let trimmed = value.text().trim();
        let name = (!trimmed.is_empty()).then(|| trimmed.to_owned());
        let effects = self.reduce(Action::RenameSession {
            operation_id: ids.operation_id(),
            name,
            at: clock.now(),
        });
        if effects
            .iter()
            .any(|effect| matches!(effect, Effect::CommitBrowserOperation(_)))
        {
            self.session_rename_persistence = SessionRenamePersistence::Saving;
        }
        effects
    }

    /// Resolve one asynchronous current-session rename without lying about persistence.
    pub fn complete_session_rename(
        &mut self,
        previous_name: Option<String>,
        result: Result<(), StoreError>,
    ) {
        self.session_rename_persistence = SessionRenamePersistence::Idle;
        match result {
            Ok(()) => self.set_success("session renamed"),
            Err(error) => {
                let _restored = self.state.board.session.rename(previous_name);
                self.set_error(format!("session rename failed: {error}"));
            }
        }
    }
}
