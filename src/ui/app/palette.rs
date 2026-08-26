//! Searchable command discovery and execution.

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
};

use super::{BoardApp, UiInput, UiKey, query::QueryEditor};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    New,
    RenameSession,
    SendSession,
    SendSessionRemove,
    Edit,
    Delete,
    Copy,
    Cut,
    Paste,
    Duplicate,
    SubmitRemove,
    SubmitKeep,
    RefreshAgents,
    RetryStorage,
    ExportRecovery,
    Undo,
    Redo,
    MoveUp,
    MoveDown,
    Collapse,
    Select,
    Help,
    Quit,
}

impl Command {
    const ALL: [(Self, &'static str); 23] = [
        (Self::New, "New thought"),
        (Self::RenameSession, "Rename session"),
        (Self::Edit, "Edit thought"),
        (Self::Delete, "Delete thought"),
        (Self::Copy, "Copy thought"),
        (Self::Cut, "Cut thought"),
        (Self::Paste, "Paste native clipboard"),
        (Self::Duplicate, "Duplicate thought or selection"),
        (
            Self::SubmitRemove,
            "Submit and remove after acceptance (default)",
        ),
        (Self::SubmitKeep, "Submit and keep thought"),
        (Self::SendSession, "Send to another Proqi session"),
        (
            Self::SendSessionRemove,
            "Send to another Proqi session and remove thought",
        ),
        (Self::RefreshAgents, "Refresh adjacent agents"),
        (Self::RetryStorage, "Retry failed save"),
        (Self::ExportRecovery, "Export recovery file"),
        (Self::Undo, "Undo board action"),
        (Self::Redo, "Redo board action"),
        (Self::MoveUp, "Move thought up"),
        (Self::MoveDown, "Move thought down"),
        (Self::Collapse, "Expand or collapse thought"),
        (Self::Select, "Toggle thought selection"),
        (Self::Help, "Open contextual help"),
        (Self::Quit, "Quit Proqi"),
    ];
}

pub(super) struct PaletteState {
    query: QueryEditor,
    selected: usize,
    scroll: usize,
    submit_supported: bool,
}

impl PaletteState {
    fn new(submit_supported: bool) -> Self {
        Self {
            query: QueryEditor::default(),
            selected: 0,
            scroll: 0,
            submit_supported,
        }
    }

    pub(super) const fn query_cursor(&self) -> usize {
        self.query.cursor()
    }

    pub(super) fn view(&self) -> (String, Vec<String>, usize) {
        (
            self.query.text().to_owned(),
            self.matches()
                .into_iter()
                .skip(self.scroll)
                .map(|(_, label)| label.to_owned())
                .collect(),
            self.selected.saturating_sub(self.scroll),
        )
    }

    pub(super) fn match_count(&self) -> usize {
        self.matches().len()
    }

    fn matches(&self) -> Vec<(Command, &'static str)> {
        let query = self.query.text().to_lowercase();
        Command::ALL
            .into_iter()
            .filter(|(command, _)| self.available(*command))
            .filter(|(_, label)| label.to_lowercase().contains(&query))
            .collect()
    }

    fn available(&self, command: Command) -> bool {
        match command {
            Command::SubmitRemove | Command::SubmitKeep => self.submit_supported,
            _ => true,
        }
    }

    fn clamp(&mut self) {
        self.selected = self.selected.min(self.match_count().saturating_sub(1));
        self.scroll = self.scroll.min(self.selected);
    }
}

impl BoardApp {
    pub(super) fn open_palette(&mut self) {
        self.help = false;
        self.search = None;
        self.palette = Some(PaletteState::new(self.supports_submission()));
    }

    pub(super) fn close_overlay(&mut self) {
        self.palette = None;
        self.search = None;
        self.transfer = None;
        self.help = false;
    }

    pub(super) fn execute_palette_index(
        &mut self,
        index: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let command = self
            .palette
            .as_ref()
            .and_then(|palette| palette.matches().get(index).copied())
            .map(|(command, _)| command);
        self.palette = None;
        command.map_or_else(Vec::new, |command| {
            self.execute_command(command, ids, clock)
        })
    }

    pub(super) fn execute_palette_visible_index(
        &mut self,
        index: usize,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let absolute = self
            .palette
            .as_ref()
            .map_or(index, |palette| palette.scroll.saturating_add(index));
        self.execute_palette_index(absolute, ids, clock)
    }

    pub(super) fn handle_palette_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let UiInput::Key(key) = input else {
            return match input {
                UiInput::Pointer(pointer) => self.handle_pointer(*pointer, ids, clock),
                UiInput::Paste(value) => self.update_palette_query(|query| query.paste(value)),
                UiInput::PasteAnnotated(payload) => {
                    self.update_palette_query(|query| query.paste(&payload.content))
                }
                UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::Key(_) => Vec::new(),
            };
        };
        match *key {
            UiKey::Escape => self.close_overlay(),
            UiKey::Enter => {
                let selected = self.palette.as_ref().map_or(0, |palette| palette.selected);
                return self.execute_palette_index(selected, ids, clock);
            }
            UiKey::Backspace => {
                if let Some(palette) = &mut self.palette {
                    palette.query.backspace();
                    palette.clamp();
                }
            }
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualUp,
                ..
            } => self.move_palette(-1),
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualDown,
                ..
            } => self.move_palette(1),
            UiKey::Move { movement, .. } => {
                if let Some(palette) = &mut self.palette {
                    palette.query.move_cursor(movement);
                }
            }
            UiKey::Delete => {
                if let Some(palette) = &mut self.palette {
                    palette.query.delete();
                    palette.clamp();
                }
            }
            UiKey::Character(character) if !character.is_control() => {
                if let Some(palette) = &mut self.palette {
                    palette.query.insert_char(character);
                    palette.selected = 0;
                    palette.scroll = 0;
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn update_palette_query(&mut self, update: impl FnOnce(&mut QueryEditor)) -> Vec<Effect> {
        if let Some(palette) = &mut self.palette {
            update(&mut palette.query);
            palette.selected = 0;
            palette.scroll = 0;
            palette.clamp();
        }
        Vec::new()
    }

    fn move_palette(&mut self, delta: isize) {
        let visible = self
            .layout
            .as_ref()
            .and_then(|layout| layout.overlay.as_ref())
            .map_or(1, |overlay| overlay.items.len().max(1));
        let Some(palette) = &mut self.palette else {
            return;
        };
        palette.selected = palette
            .selected
            .saturating_add_signed(delta)
            .min(palette.match_count().saturating_sub(1));
        if palette.selected < palette.scroll {
            palette.scroll = palette.selected;
        } else if palette.selected >= palette.scroll.saturating_add(visible) {
            palette.scroll = palette.selected + 1 - visible;
        }
    }

    fn execute_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match command {
            Command::New => self.create(crate::ui::PastePayload::text(String::new()), ids, clock),
            Command::RenameSession => {
                self.begin_session_rename();
                Vec::new()
            }
            Command::SendSession => self.begin_session_transfer(false, ids, clock),
            Command::SendSessionRemove => self.begin_session_transfer(true, ids, clock),
            Command::Edit => {
                self.enter_edit();
                Vec::new()
            }
            Command::Delete => self.delete(ids, clock),
            Command::Copy => self.copy_active(ids),
            Command::Cut => self.cut_active(ids, clock),
            Command::Paste => self.read_clipboard(ids),
            Command::Duplicate => self.duplicate(ids, clock),
            Command::SubmitRemove => self.begin_delivery(
                crate::ports::agent::SubmissionDisposition::RemoveAfterSuccess,
                ids,
                clock,
            ),
            Command::SubmitKeep => {
                self.begin_delivery(crate::ports::agent::SubmissionDisposition::Keep, ids, clock)
            }
            Command::RefreshAgents => self.refresh_agents(),
            Command::RetryStorage => self.retry_persistence(),
            Command::ExportRecovery => self.export_recovery(ids, clock),
            Command::Undo => self.history(ids, clock, true),
            Command::Redo => self.history(ids, clock, false),
            Command::MoveUp => self.reorder(ids, clock, -1),
            Command::MoveDown => self.reorder(ids, clock, 1),
            Command::Collapse => self.collapse(ids, clock),
            Command::Select => {
                self.toggle_selection();
                Vec::new()
            }
            Command::Help => {
                self.help = true;
                Vec::new()
            }
            Command::Quit => {
                self.request_quit();
                Vec::new()
            }
        }
    }
}
