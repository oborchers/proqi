//! Shortcut-registry dispatch, query editing, selection, and pointer activation.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::ui::{PointerButton, PointerInput, PointerKind, UiInput, UiKey};

use super::{BrowserAction, BrowserAvailability, BrowserHit, SessionBrowser, SessionBrowserItem};

impl SessionBrowser {
    /// Apply one normalized terminal event.
    pub fn handle(&mut self, input: UiInput) -> BrowserAction {
        self.status = None;
        let Some(input) = self.resolve_shortcut_input(input) else {
            return BrowserAction::Continue;
        };
        if self.rename.is_some() {
            return self.handle_rename(input);
        }
        self.handle_resolved_input(input)
    }

    fn resolve_shortcut_input(&self, input: UiInput) -> Option<UiInput> {
        let context = if self.rename.is_some() {
            crate::ui::ShortcutContext::BrowserRename
        } else if self.query.is_empty() {
            crate::ui::ShortcutContext::Browser
        } else {
            crate::ui::ShortcutContext::BrowserQuery
        };
        let contexts = crate::ui::ShortcutContextStack::new([context]);
        match input {
            UiInput::KeyStroke(stroke) => self
                .shortcut_registry
                .dispatch(&contexts, stroke)
                .map(|resolved| UiInput::Key(resolved.intention)),
            UiInput::Key(key) => Some(UiInput::Key(
                self.shortcut_registry
                    .normalize_existing_intention(&contexts, key),
            )),
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
            UiInput::Key(UiKey::Backspace | UiKey::Delete | UiKey::ModifiedDelete) => {
                if let Some((index, _)) = self.query.grapheme_indices(true).next_back() {
                    self.query.truncate(index);
                }
                self.refilter();
                BrowserAction::Continue
            }
            UiInput::Key(UiKey::Move { movement, .. }) => {
                use crate::ports::editor::CursorMovement;
                match movement {
                    CursorMovement::VisualUp
                    | CursorMovement::GraphemeBack
                    | CursorMovement::WordBack
                    | CursorMovement::LineStart
                    | CursorMovement::DocumentStart => self.move_selection(-1),
                    _ => self.move_selection(1),
                }
                BrowserAction::Continue
            }
            UiInput::Key(UiKey::Character(character)) => {
                self.query.push(character);
                self.refilter();
                BrowserAction::Continue
            }
            UiInput::Key(UiKey::Shortcut(crate::ui::ShortcutActionId::RenameSession)) => {
                self.begin_rename()
            }
            UiInput::Key(UiKey::Shortcut(crate::ui::ShortcutActionId::BrowserTrash)) => {
                self.trash_selected()
            }
            UiInput::Key(UiKey::UnmodifiedSpace) => {
                self.query.push(' ');
                self.refilter();
                BrowserAction::Continue
            }
            UiInput::Paste(text) => {
                self.query.push_str(&text.replace(['\r', '\n'], " "));
                self.refilter();
                BrowserAction::Continue
            }
            UiInput::PasteAnnotated(payload) => {
                self.query
                    .push_str(&payload.content.replace(['\r', '\n'], " "));
                self.refilter();
                BrowserAction::Continue
            }
            UiInput::Pointer(pointer) => self.handle_pointer(pointer),
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::KeyStroke(_)
            | UiInput::Key(_) => BrowserAction::Continue,
        }
    }

    fn refilter(&mut self) {
        let query = self.query.to_lowercase();
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
        match layout.hit_test(pointer.column, pointer.row) {
            BrowserHit::Cancel => BrowserAction::Cancel,
            BrowserHit::Rename => self.begin_rename(),
            BrowserHit::Trash => self.trash_selected(),
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
