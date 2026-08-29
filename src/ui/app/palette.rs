//! Searchable command discovery and execution.

mod command;

use crate::{
    application::Effect,
    ports::{
        editor::CursorMovement,
        environment::{Clock, IdGenerator},
    },
};

use super::{
    BoardApp, UiInput, UiKey, palette_handoff::EditorSelectionHandoff, query::QueryEditor,
};

use command::Command;

pub(super) struct PaletteState {
    query: QueryEditor,
    selected: usize,
    scroll: usize,
    submit_supported: bool,
    plain_newline_supported: bool,
    selection_handoff: Option<EditorSelectionHandoff>,
}

impl PaletteState {
    fn new(
        submit_supported: bool,
        plain_newline_supported: bool,
        selection_handoff: Option<EditorSelectionHandoff>,
    ) -> Self {
        Self {
            query: QueryEditor::default(),
            selected: 0,
            scroll: 0,
            submit_supported,
            plain_newline_supported,
            selection_handoff,
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
            Command::SubmitRemove
            | Command::SubmitKeep
            | Command::SubmitAllRemove
            | Command::SubmitAllKeep => self.submit_supported,
            Command::PlainNewline
            | Command::JumpUp
            | Command::JumpDown
            | Command::ThoughtStart
            | Command::ThoughtEnd
            | Command::Indent
            | Command::Outdent => self.plain_newline_supported,
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
        self.deactivate_range_latch();
        self.help = false;
        self.search = None;
        self.palette = Some(PaletteState::new(
            self.supports_submission(),
            !self.insertion_focused() && self.state.focused_thought.is_some(),
            self.palette_selection_handoff.take(),
        ));
    }

    pub(super) fn close_overlay(&mut self) {
        self.palette = None;
        self.search = None;
        self.transfer = None;
        self.invocation_popup = None;
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
        let selection_handoff = self
            .palette
            .as_mut()
            .and_then(|palette| palette.selection_handoff.take());
        self.palette = None;
        command.map_or_else(Vec::new, |command| {
            self.execute_command(command, selection_handoff, ids, clock)
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
        selection_handoff: Option<EditorSelectionHandoff>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if let Some(effects) = self.execute_submission_command(command, ids, clock) {
            return effects;
        }
        if let Some(effects) = self.execute_editor_command(command, selection_handoff, ids, clock) {
            return effects;
        }
        if let Some(effects) = self.execute_entry_command(command, ids, clock) {
            return effects;
        }
        match command {
            Command::New => self.create(crate::ui::PastePayload::text(String::new()), ids, clock),
            Command::RenameSession => {
                self.begin_session_rename();
                Vec::new()
            }
            Command::CopySessionId => self.copy_session_id(ids),
            Command::CopyResume => self.copy_resume_command(ids),
            Command::SendSession => self.begin_session_transfer(false, ids, clock),
            Command::SendSessionRemove => self.begin_session_transfer(true, ids, clock),
            Command::Delete => self.delete(ids, clock),
            Command::Copy => self.copy_active(ids),
            Command::Cut => self.cut_active(ids, clock),
            Command::Paste => self.read_clipboard(ids),
            Command::Duplicate => self.duplicate(ids, clock),
            Command::SelectAll => {
                let effects = if matches!(
                    self.state.mode,
                    crate::application::InteractionMode::Edit { .. }
                ) {
                    self.finish_edit(ids, clock)
                } else {
                    Vec::new()
                };
                self.select_all_thoughts();
                effects
            }
            Command::SubmitRemove
            | Command::SubmitKeep
            | Command::SubmitAllRemove
            | Command::SubmitAllKeep
            | Command::PlainNewline
            | Command::JumpUp
            | Command::JumpDown
            | Command::ThoughtStart
            | Command::ThoughtEnd
            | Command::Indent
            | Command::Outdent
            | Command::Edit
            | Command::InsertInvocation => Vec::new(),
            Command::RefreshAgents => self.refresh_agents(),
            Command::RefreshInvocations => self.refresh_invocations(),
            Command::CheckUpdates => {
                vec![Effect::Update(crate::application::UpdateIntent::CheckNow)]
            }
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
            Command::RangeSelect => {
                self.activate_range_latch();
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

    fn execute_entry_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        match command {
            Command::Edit => Some(self.expand_and_enter_edit(ids, clock)),
            Command::InsertInvocation => {
                let effects =
                    if matches!(self.state.mode, crate::application::InteractionMode::Board) {
                        self.expand_and_enter_edit(ids, clock)
                    } else {
                        Vec::new()
                    };
                self.open_invocation_picker();
                Some(effects)
            }
            _ => None,
        }
    }

    fn execute_submission_command(
        &mut self,
        command: Command,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        use crate::ports::agent::SubmissionDisposition::{Keep, RemoveAfterSuccess};
        match command {
            Command::SubmitRemove => Some(self.begin_delivery(RemoveAfterSuccess, ids, clock)),
            Command::SubmitKeep => Some(self.begin_delivery(Keep, ids, clock)),
            Command::SubmitAllRemove => {
                Some(self.begin_delivery_all(RemoveAfterSuccess, ids, clock))
            }
            Command::SubmitAllKeep => Some(self.begin_delivery_all(Keep, ids, clock)),
            _ => None,
        }
    }

    fn execute_editor_command(
        &mut self,
        command: Command,
        selection_handoff: Option<EditorSelectionHandoff>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        if !matches!(
            command,
            Command::PlainNewline
                | Command::JumpUp
                | Command::JumpDown
                | Command::ThoughtStart
                | Command::ThoughtEnd
                | Command::Indent
                | Command::Outdent
        ) {
            return None;
        }
        let mut effects = if matches!(
            self.state.mode,
            crate::application::InteractionMode::Edit { .. }
        ) {
            Vec::new()
        } else {
            self.expand_and_enter_edit(ids, clock)
        };
        if command == Command::PlainNewline {
            effects.extend(self.insert_newline(false, ids, clock));
            return Some(effects);
        }
        let movement = match command {
            Command::JumpUp => Some(CursorMovement::VisualJumpUp),
            Command::JumpDown => Some(CursorMovement::VisualJumpDown),
            Command::ThoughtStart => Some(CursorMovement::DocumentStart),
            Command::ThoughtEnd => Some(CursorMovement::DocumentEnd),
            _ => None,
        };
        if let Some(movement) = movement {
            effects.extend(self.handle_edit_key(
                UiKey::Move {
                    movement,
                    extend_selection: false,
                },
                ids,
                clock,
            ));
            return Some(effects);
        }
        self.restore_palette_selection_handoff(selection_handoff);
        effects.extend(self.apply_indentation(command == Command::Outdent, ids, clock));
        Some(effects)
    }
}
