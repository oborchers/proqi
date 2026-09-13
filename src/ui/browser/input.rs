//! Shortcut-registry dispatch, query editing, selection, and pointer activation.

use crate::ui::input::{RoutedInput as UiInput, UiKey};
use crate::ui::{PointerButton, PointerInput, PointerKind, UiInput as ExternalInput};

use super::{BrowserAction, BrowserAvailability, BrowserHit, SessionBrowser, SessionBrowserItem};

impl SessionBrowser {
    /// Apply one normalized terminal event.
    pub fn handle(&mut self, input: ExternalInput) -> BrowserAction {
        self.status = None;
        let input = UiInput::from(input);
        let Some(input) = self.resolve_shortcut_input(input) else {
            return BrowserAction::Continue;
        };
        if self.rename.is_some() {
            return self.handle_rename(input);
        }
        self.handle_resolved_input(input)
    }

    pub(in crate::ui) fn shortcut_context(&self) -> crate::ui::ShortcutContext {
        if self.rename.is_some() {
            crate::ui::ShortcutContext::BrowserRename
        } else if self.query.text().is_empty() {
            crate::ui::ShortcutContext::Browser
        } else {
            crate::ui::ShortcutContext::BrowserQuery
        }
    }

    fn resolve_shortcut_input(&self, input: UiInput) -> Option<UiInput> {
        let contexts = crate::ui::ShortcutContextStack::new([self.shortcut_context()]);
        match input {
            UiInput::KeyStroke(stroke) => self
                .shortcut_registry
                .dispatch(&contexts, stroke)
                .map(|resolved| UiInput::Key(resolved.intention)),
            UiInput::Key(key) => Some(UiInput::Key(key)),
            input => Some(input),
        }
    }

    fn handle_resolved_input(&mut self, input: UiInput) -> BrowserAction {
        match input {
            UiInput::Key(UiKey::Quit | UiKey::Escape) => BrowserAction::Cancel,
            UiInput::Key(UiKey::Enter) => self.activate(),
            UiInput::Key(UiKey::FastNavigation { direction, .. }) => {
                self.selected = direction.move_index(self.selected, self.filtered.len());
                self.layout = None;
                BrowserAction::Continue
            }
            UiInput::Key(UiKey::Move {
                movement,
                extend_selection,
            }) => self.handle_movement(movement, extend_selection),
            UiInput::Key(UiKey::Shortcut(crate::ui::ShortcutActionId::RenameSession)) => {
                self.begin_rename()
            }
            UiInput::Key(UiKey::Shortcut(crate::ui::ShortcutActionId::BrowserTrash)) => {
                self.trash_selected()
            }
            UiInput::Key(UiKey::Undo) => self.handle_history(true),
            UiInput::Key(UiKey::Redo) => self.handle_history(false),
            UiInput::Pointer(pointer) => self.handle_pointer(pointer),
            input @ (UiInput::Key(
                UiKey::Backspace
                | UiKey::Delete
                | UiKey::ModifiedDelete
                | UiKey::Character(_)
                | UiKey::UnmodifiedSpace
                | UiKey::SelectAll,
            )
            | UiInput::Paste(_)
            | UiInput::PasteAnnotated(_)) => self.edit_query(input),
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::KeyStroke(_)
            | UiInput::Key(_) => BrowserAction::Continue,
        }
    }

    fn handle_movement(
        &mut self,
        movement: crate::ports::editor::CursorMovement,
        extend_selection: bool,
    ) -> BrowserAction {
        use crate::ports::editor::CursorMovement;
        match movement {
            CursorMovement::VisualUp | CursorMovement::VisualJumpUp => self.move_selection(-1),
            CursorMovement::VisualDown | CursorMovement::VisualJumpDown => self.move_selection(1),
            _ => self
                .query
                .move_cursor_with_selection(movement, extend_selection),
        }
        BrowserAction::Continue
    }

    fn handle_history(&mut self, undo: bool) -> BrowserAction {
        if self.query.can_undo() || self.query.can_redo() {
            if undo {
                self.query.undo();
            } else {
                self.query.redo();
            }
            self.refilter();
            return BrowserAction::Continue;
        }
        if let Some(target) = self.history_target(undo) {
            BrowserAction::History { undo, target }
        } else {
            let direction = if undo { "undo" } else { "redo" };
            self.status = Some(format!("Nothing to {direction} in Browser history"));
            BrowserAction::Continue
        }
    }

    fn edit_query(&mut self, input: UiInput) -> BrowserAction {
        let content_changed = match input {
            UiInput::Key(UiKey::Backspace) => {
                self.query.backspace();
                true
            }
            UiInput::Key(UiKey::Delete | UiKey::ModifiedDelete) => {
                self.query.delete();
                true
            }
            UiInput::Key(UiKey::Character(character)) => {
                self.query.insert_char(character);
                true
            }
            UiInput::Key(UiKey::UnmodifiedSpace) => {
                self.query.insert_char(' ');
                true
            }
            UiInput::Paste(text) => {
                self.query.paste(&text);
                true
            }
            UiInput::PasteAnnotated(payload) => {
                self.query.paste(&payload.content);
                true
            }
            UiInput::Key(UiKey::SelectAll) => {
                self.query.select_all();
                false
            }
            _ => false,
        };
        if content_changed {
            self.refilter();
        }
        BrowserAction::Continue
    }

    fn refilter(&mut self) {
        let query = self.query.text().to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let searchable = searchable_text(item).to_lowercase();
                query
                    .split_whitespace()
                    .all(|word| searchable.contains(word))
                    .then_some(index)
            })
            .collect();
        self.selected = 0;
        self.first_visible = 0;
        self.layout = None;
    }

    fn move_selection(&mut self, amount: isize) {
        let last = self.filtered.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(amount).min(last);
        self.layout = None;
    }

    fn handle_pointer(&mut self, pointer: PointerInput) -> BrowserAction {
        if matches!(
            pointer.kind,
            PointerKind::ScrollUp | PointerKind::ScrollDown
        ) {
            self.move_selection(if matches!(pointer.kind, PointerKind::ScrollUp) {
                -1
            } else {
                1
            });
            return BrowserAction::Continue;
        }
        if !matches!(pointer.kind, PointerKind::Down(PointerButton::Left)) {
            return BrowserAction::Continue;
        }
        let Some(layout) = &self.layout else {
            return BrowserAction::Continue;
        };
        match layout.hit_test(pointer.column, pointer.row, &self.footer_controls) {
            BrowserHit::Cancel => BrowserAction::Cancel,
            BrowserHit::Rename => self.begin_rename(),
            BrowserHit::Trash => self.trash_selected(),
            BrowserHit::Undo => self.handle_resolved_input(UiInput::Key(UiKey::Undo)),
            BrowserHit::Redo => self.handle_resolved_input(UiInput::Key(UiKey::Redo)),
            BrowserHit::Confirm => self.activate(),
            BrowserHit::Item(item_index) => {
                let Some(position) = self.filtered.iter().position(|index| *index == item_index)
                else {
                    return BrowserAction::Continue;
                };
                self.selected = position;
                self.activate()
            }
            BrowserHit::None => BrowserAction::Continue,
        }
    }

    fn activate(&mut self) -> BrowserAction {
        let Some((_, item)) = self.selected_item() else {
            self.status = Some("No matching session".to_owned());
            return BrowserAction::Continue;
        };
        match &item.availability {
            BrowserAvailability::Resumable | BrowserAvailability::Recovered => {
                BrowserAction::Open(item.hit.id)
            }
            BrowserAvailability::Active(instance) => {
                self.status = Some(format!("Session is active in process {}", instance.pid));
                BrowserAction::Continue
            }
            BrowserAvailability::Trashed => {
                self.status = Some("Restore this session before opening it".to_owned());
                BrowserAction::Continue
            }
        }
    }
}

fn searchable_text(item: &SessionBrowserItem) -> String {
    let mut values = vec![
        item.hit.id.to_string(),
        item.hit.name.clone().unwrap_or_default(),
        item.hit.origin_cwd.to_string_lossy().into_owned(),
        item.hit.last_opened_cwd.to_string_lossy().into_owned(),
        item.hit.excerpt.clone(),
        item.hit.search_content.clone(),
    ];
    values.extend(item.hit.previews.iter().cloned());
    if let Some(context) = &item.hit.integration_context {
        values.extend([
            context.provider.clone(),
            context.agent_kind.clone(),
            context.agent_name.clone(),
            context.workspace_hint.clone().unwrap_or_default(),
            context.tab_hint.clone().unwrap_or_default(),
            context.pane_hint.clone().unwrap_or_default(),
        ]);
    }
    values.join("\n")
}
