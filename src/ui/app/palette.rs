//! Searchable command discovery and execution.

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
};

use super::{BoardApp, UiInput, UiKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    New,
    Edit,
    Delete,
    Undo,
    Redo,
    MoveUp,
    MoveDown,
    Collapse,
    Help,
    Quit,
}

impl Command {
    const ALL: [(Self, &'static str); 10] = [
        (Self::New, "New thought"),
        (Self::Edit, "Edit focused thought"),
        (Self::Delete, "Delete focused thought"),
        (Self::Undo, "Undo board action"),
        (Self::Redo, "Redo board action"),
        (Self::MoveUp, "Move thought up"),
        (Self::MoveDown, "Move thought down"),
        (Self::Collapse, "Expand or collapse thought"),
        (Self::Help, "Open contextual help"),
        (Self::Quit, "Quit Proqi"),
    ];
}

pub(super) struct PaletteState {
    query: String,
    selected: usize,
}

impl PaletteState {
    fn new() -> Self {
        Self {
            query: String::new(),
            selected: 0,
        }
    }

    pub(super) fn view(&self) -> (String, Vec<&'static str>, usize) {
        (
            self.query.clone(),
            self.matches().into_iter().map(|(_, label)| label).collect(),
            self.selected,
        )
    }

    pub(super) fn match_count(&self) -> usize {
        self.matches().len()
    }

    fn matches(&self) -> Vec<(Command, &'static str)> {
        let query = self.query.to_lowercase();
        Command::ALL
            .into_iter()
            .filter(|(_, label)| label.to_lowercase().contains(&query))
            .collect()
    }

    fn clamp(&mut self) {
        self.selected = self.selected.min(self.match_count().saturating_sub(1));
    }
}

impl BoardApp {
    pub(super) fn open_palette(&mut self) {
        self.help = false;
        self.palette = Some(PaletteState::new());
    }

    pub(super) fn close_overlay(&mut self) {
        self.palette = None;
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

    pub(super) fn handle_palette_input(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let UiInput::Key(key) = input else {
            return match input {
                UiInput::Pointer(pointer) => self.handle_pointer(*pointer, ids, clock),
                UiInput::Resize { .. } | UiInput::Paste(_) | UiInput::Key(_) => Vec::new(),
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
                    palette.query.pop();
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
            UiKey::Character(character) if !character.is_control() => {
                if let Some(palette) = &mut self.palette {
                    palette.query.push(character);
                    palette.selected = 0;
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn move_palette(&mut self, delta: isize) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        palette.selected = palette
            .selected
            .saturating_add_signed(delta)
            .min(palette.match_count().saturating_sub(1));
    }

    fn execute_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match command {
            Command::New => self.create(String::new(), ids, clock),
            Command::Edit => {
                self.enter_edit();
                Vec::new()
            }
            Command::Delete => self.delete(ids, clock),
            Command::Undo => self.history(ids, clock, true),
            Command::Redo => self.history(ids, clock, false),
            Command::MoveUp => self.reorder(ids, clock, -1),
            Command::MoveDown => self.reorder(ids, clock, 1),
            Command::Collapse => self.collapse(ids, clock),
            Command::Help => {
                self.help = true;
                Vec::new()
            }
            Command::Quit => {
                self.quit = true;
                Vec::new()
            }
        }
    }
}
