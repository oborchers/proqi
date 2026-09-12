//! Searchable command discovery and execution.

mod binding;
pub(super) mod command;
mod dispatch;
mod editor;
mod invocation;

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
};

use super::{
    BoardApp, UiInput, UiKey, palette_handoff::EditorSelectionHandoff, query::QueryEditor,
};
use crate::ui::{CommandMetadata, shortcut_registry::CommandExecution};

use command::Command;
use invocation::CommandInvocation;

pub(super) struct PaletteState {
    commands: Vec<(Command, CommandMetadata, CommandExecution)>,
    query: QueryEditor,
    selected: usize,
    scroll: usize,
    invocation: CommandInvocation,
}

impl PaletteState {
    fn new(
        commands: Vec<(Command, CommandMetadata, CommandExecution)>,
        invocation: CommandInvocation,
    ) -> Self {
        Self {
            commands,
            query: QueryEditor::default(),
            selected: 0,
            scroll: 0,
            invocation,
        }
    }

    pub(super) const fn query_cursor(&self) -> usize {
        self.query.cursor()
    }

    pub(super) const fn query_selection(&self) -> Option<super::query::QuerySelection> {
        self.query.selection()
    }

    pub(super) fn view(&self) -> (String, Vec<String>, usize) {
        (
            self.query.text().to_owned(),
            self.matches()
                .into_iter()
                .skip(self.scroll)
                .map(|(_, label, _)| label.to_owned())
                .collect(),
            self.selected.saturating_sub(self.scroll),
        )
    }

    pub(super) fn match_count(&self) -> usize {
        self.matches().len()
    }

    pub(super) fn overflow(&self, visible: usize) -> (bool, bool) {
        (
            self.scroll > 0,
            self.scroll.saturating_add(visible) < self.match_count(),
        )
    }

    fn matches(&self) -> Vec<(Command, &'static str, CommandExecution)> {
        let query = self.query.text().to_lowercase();
        self.commands
            .iter()
            .copied()
            .filter(|(_, metadata, _)| match metadata.availability {
                crate::ui::CommandAvailability::QueryUndo => self.query.can_undo(),
                crate::ui::CommandAvailability::QueryRedo => self.query.can_redo(),
                other => self.invocation.available(other),
            })
            .map(|(command, metadata, execution)| {
                (
                    command,
                    self.invocation.command_label(metadata.label),
                    execution,
                )
            })
            .filter(|(_, label, _)| label.to_lowercase().contains(&query))
            .collect()
    }

    fn clamp(&mut self) {
        self.selected = self.selected.min(self.match_count().saturating_sub(1));
        self.scroll = self.scroll.min(self.selected);
    }
}

impl BoardApp {
    pub(super) fn refresh_screenshot_palette_action(&mut self) {
        let action = self.screenshot_palette_action();
        if let Some(palette) = &mut self.palette {
            palette.invocation.set_screenshot_action(action);
            palette.clamp();
        }
    }

    pub(super) fn open_palette(&mut self) {
        self.deactivate_range_latch();
        self.help = false;
        self.search = None;
        let commands = self.settings.shortcuts.commands();
        let invocation = self.capture_command_invocation();
        self.palette = Some(PaletteState::new(commands, invocation));
    }

    pub(super) fn close_overlay(&mut self) {
        self.cancel_screenshot_takeover();
        self.palette = None;
        self.global_delivery = None;
        self.search = None;
        self.transfer = None;
        self.close_invocation_picker();
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
            .map(|(_, _, execution)| execution);
        if let Some(undo) = palette_query_history(command) {
            return self.update_palette_query(|query| move_query_history(query, undo));
        }
        let selection_handoff = self
            .palette
            .as_mut()
            .and_then(|palette| palette.invocation.take_selection_handoff());
        let merge_handoff = self
            .palette
            .as_mut()
            .and_then(|palette| palette.invocation.take_merge_handoff());
        self.palette = None;
        command.map_or_else(Vec::new, |execution| {
            self.execute_command(
                execution,
                selection_handoff,
                merge_handoff.as_deref(),
                ids,
                clock,
            )
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
        match input {
            UiInput::Key(key) => self.handle_palette_key(*key, ids, clock),
            input => self.handle_palette_non_key(input, ids, clock),
        }
    }

    fn handle_palette_key(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match key {
            UiKey::Shortcut(action) => return self.execute_bound_command(action, ids, clock),
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
            UiKey::FastNavigation { direction, .. } => self.move_palette(direction.delta()),
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualUp,
                ..
            } => self.move_palette(-1),
            UiKey::Move {
                movement: crate::ports::editor::CursorMovement::VisualDown,
                ..
            } => self.move_palette(1),
            UiKey::Move {
                movement,
                extend_selection,
            } => {
                if let Some(palette) = &mut self.palette {
                    palette
                        .query
                        .move_cursor_with_selection(movement, extend_selection);
                }
            }
            UiKey::Delete | UiKey::ModifiedDelete => {
                if let Some(palette) = &mut self.palette {
                    palette.query.delete();
                    palette.clamp();
                }
            }
            UiKey::Character(character) if !character.is_control() => {
                return self.update_palette_query(|query| query.insert_char(character));
            }
            UiKey::UnmodifiedSpace => {
                return self.update_palette_query(|query| query.insert_char(' '));
            }
            UiKey::SelectAll => {
                if let Some(palette) = &mut self.palette {
                    palette.query.select_all();
                }
            }
            UiKey::Undo => {
                if let Some(palette) = &mut self.palette {
                    palette.query.undo();
                    palette.clamp();
                }
            }
            UiKey::Redo => {
                if let Some(palette) = &mut self.palette {
                    palette.query.redo();
                    palette.clamp();
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_palette_non_key(
        &mut self,
        input: &UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::Pointer(pointer) => match pointer.kind {
                crate::ui::PointerKind::ScrollUp => {
                    self.move_palette(-1);
                    Vec::new()
                }
                crate::ui::PointerKind::ScrollDown => {
                    self.move_palette(1);
                    Vec::new()
                }
                _ => self.handle_pointer(*pointer, ids, clock),
            },
            UiInput::Paste(value) => self.update_palette_query(|query| query.paste(value)),
            UiInput::PasteAnnotated(payload) => {
                self.update_palette_query(|query| query.paste(&payload.content))
            }
            UiInput::Resize { .. }
            | UiInput::HostFocusGained
            | UiInput::HostFocusLost
            | UiInput::KeyStroke(_)
            | UiInput::Key(_) => Vec::new(),
        }
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
        palette.scroll =
            crate::ui::paging::first_visible(palette.selected, palette.scroll, visible);
        self.layout = None;
    }

    pub(super) fn ensure_palette_visible(&mut self, visible: usize) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        palette.selected = palette
            .selected
            .min(palette.match_count().saturating_sub(1));
        palette.scroll =
            crate::ui::paging::first_visible(palette.selected, palette.scroll, visible);
    }

    fn execute_command(
        &mut self,
        execution: CommandExecution,
        selection_handoff: Option<EditorSelectionHandoff>,
        merge_handoff: Option<&[crate::domain::Thought]>,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        use CommandExecution as Execution;

        let acknowledges_auto_pause = execution.acknowledges_screenshot_auto_pause();
        self.acknowledge_screenshot_auto_pause_warning(acknowledges_auto_pause);
        self.clear_status_for_interaction(acknowledges_auto_pause);
        match execution {
            Execution::ReflowThought => self.reflow_thought_in_place(ids, clock),
            Execution::Paste(command) => {
                self.execute_palette_paste(command, selection_handoff, ids, clock)
            }
            Execution::Transformation(command) => self.execute_transformation_command(
                command,
                selection_handoff.as_ref(),
                merge_handoff,
                ids,
                clock,
            ),
            Execution::Submission(command) => self.execute_submission_command(command, ids, clock),
            Execution::Editor(command) => {
                self.execute_editor_command(command, selection_handoff, ids, clock)
            }
            Execution::Entry(command) => self.execute_entry_command(command, ids, clock),
            Execution::Selection(command) => self.execute_selection_command(command, ids, clock),
            Execution::Runtime(command) => self.execute_runtime_command(command, ids, clock),
            Execution::Board(command) => self.execute_board_command(command, ids, clock),
        }
    }

    fn execute_board_command(
        &mut self,
        command: crate::ui::shortcut_registry::PaletteBoardCommand,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        use crate::ui::shortcut_registry::PaletteBoardCommand as BoardCommand;
        match command {
            BoardCommand::New => {
                self.create(crate::ui::PastePayload::text(String::new()), ids, clock)
            }
            BoardCommand::InsertAbove => self.insert_relative_to_focus(false, ids, clock),
            BoardCommand::InsertBelow => self.insert_relative_to_focus(true, ids, clock),
            BoardCommand::RenameSession => {
                self.begin_session_rename();
                Vec::new()
            }
            BoardCommand::CopySessionId => self.copy_session_id(ids),
            BoardCommand::CopyResume => self.copy_resume_command(ids),
            BoardCommand::SendSession => self.begin_session_transfer(false, ids, clock),
            BoardCommand::SendSessionRemove => self.begin_session_transfer(true, ids, clock),
            BoardCommand::Delete => self.delete(ids, clock),
            BoardCommand::Copy => self.copy_active(ids),
            BoardCommand::Cut => self.cut_active(ids, clock),
            BoardCommand::Duplicate => self.duplicate(ids, clock),
            BoardCommand::Undo => self.history(ids, clock, true),
            BoardCommand::Redo => self.history(ids, clock, false),
            BoardCommand::MoveUp => self.reorder(ids, clock, -1),
            BoardCommand::MoveDown => self.reorder(ids, clock, 1),
            BoardCommand::FocusFirst => {
                self.focus_thought_boundary(false);
                Vec::new()
            }
            BoardCommand::FocusLast => {
                self.focus_thought_boundary(true);
                Vec::new()
            }
            BoardCommand::Collapse => self.collapse(ids, clock),
            BoardCommand::Help => {
                self.help = true;
                Vec::new()
            }
            BoardCommand::Quit => self.request_quit_after_edit_flush(ids, clock),
        }
    }
}

fn palette_query_history(command: Option<CommandExecution>) -> Option<bool> {
    use crate::ui::shortcut_registry::PaletteBoardCommand;

    match command {
        Some(CommandExecution::Board(PaletteBoardCommand::Undo)) => Some(true),
        Some(CommandExecution::Board(PaletteBoardCommand::Redo)) => Some(false),
        _ => None,
    }
}

fn move_query_history(query: &mut QueryEditor, undo: bool) {
    if undo {
        query.undo();
    } else {
        query.redo();
    }
}
