//! Searchable command discovery and execution.

mod binding;
mod dispatch;
mod editor;
mod invocation;
mod projection;
mod ranking;
mod refresh;

use crate::{
    application::Effect,
    ports::environment::{Clock, IdGenerator},
};

use super::{
    BoardApp, UiInput, UiKey, palette_handoff::EditorSelectionHandoff, query::QueryEditor,
};
use crate::ui::shortcut_registry::CommandExecution;

use invocation::CommandContext;
use projection::{CommandRecord, ProjectedRow, RowAction};

pub(in crate::ui) struct PaletteRowView {
    pub(in crate::ui) primary: String,
    pub(in crate::ui) secondary: Option<String>,
    pub(in crate::ui) secondary_fallbacks: Vec<String>,
    pub(in crate::ui) protected_secondaries: Vec<String>,
    pub(in crate::ui) group: Option<&'static str>,
    pub(in crate::ui) enabled: bool,
}

pub(in crate::ui) struct CommandPaletteView {
    pub(in crate::ui) query: String,
    pub(in crate::ui) rows: Vec<PaletteRowView>,
    pub(in crate::ui) selected: usize,
}

pub(super) struct PaletteState {
    commands: Vec<CommandRecord>,
    query: QueryEditor,
    selected: usize,
    scroll: usize,
    expanded: bool,
    context: CommandContext,
}

impl PaletteState {
    fn new(registry: &crate::ui::ShortcutRegistry, context: CommandContext) -> Self {
        let binding_context = context.shortcut_context();
        let commands = registry
            .commands()
            .into_iter()
            .map(|(action, metadata, execution)| {
                let shortcut = registry.compact_help_label(binding_context, &[action]);
                CommandRecord {
                    action,
                    metadata,
                    execution,
                    shortcut: (!shortcut.is_empty()).then_some(shortcut),
                }
            })
            .collect();
        let mut state = Self {
            commands,
            query: QueryEditor::default(),
            selected: 0,
            scroll: 0,
            expanded: false,
            context,
        };
        state.clamp();
        state
    }

    pub(super) const fn query_cursor(&self) -> usize {
        self.query.cursor()
    }

    pub(super) fn view(&self) -> (String, Vec<String>, usize) {
        let rows = self.projected();
        (
            self.query.text().to_owned(),
            rows.into_iter()
                .skip(self.scroll)
                .map(|row| row.primary)
                .collect(),
            self.selected.saturating_sub(self.scroll),
        )
    }

    pub(super) fn command_view(&self) -> CommandPaletteView {
        let rows = self
            .projected()
            .into_iter()
            .skip(self.scroll)
            .map(|row| PaletteRowView {
                primary: row.primary,
                secondary: row.secondary,
                secondary_fallbacks: row.secondary_fallbacks,
                protected_secondaries: row.protected_secondaries,
                group: row.group,
                enabled: row.enabled,
            })
            .collect();
        CommandPaletteView {
            query: self.query.text().to_owned(),
            rows,
            selected: self.selected.saturating_sub(self.scroll),
        }
    }

    pub(super) fn match_count(&self) -> usize {
        self.projected().len()
    }

    pub(super) fn visible_row_metadata(&self) -> (Vec<bool>, Vec<bool>) {
        let rows = self.projected();
        let visible = rows.into_iter().skip(self.scroll);
        let pairs = visible
            .map(|row| (row.group.is_some(), row.selectable()))
            .collect::<Vec<_>>();
        (
            pairs.iter().map(|(group, _)| *group).collect(),
            pairs.iter().map(|(_, interactive)| *interactive).collect(),
        )
    }

    pub(super) fn preferred_rows(&self) -> usize {
        let rows = self.projected();
        rows.len()
            .saturating_add(rows.iter().filter(|row| row.group.is_some()).count())
            .max(2)
    }

    pub(super) fn overflow(&self, visible: usize) -> (bool, bool) {
        (
            self.scroll > 0,
            self.scroll.saturating_add(visible) < self.match_count(),
        )
    }

    fn projected(&self) -> Vec<ProjectedRow> {
        projection::rows(
            &self.commands,
            &self.context,
            self.query.text(),
            self.expanded,
        )
    }

    fn clamp(&mut self) {
        let rows = self.projected();
        self.selected = self.selected.min(rows.len().saturating_sub(1));
        if !rows
            .get(self.selected)
            .is_some_and(ProjectedRow::selectable)
        {
            self.selected = rows.iter().position(ProjectedRow::selectable).unwrap_or(0);
        }
        self.scroll = self.scroll.min(self.selected);
    }

    fn refresh_context(&mut self, update: impl FnOnce(&mut CommandContext)) {
        let selected_action = self.projected().get(self.selected).map(|row| row.action);
        let selected_offset = self.selected.saturating_sub(self.scroll);
        update(&mut self.context);
        let rows = self.projected();
        if let Some(index) = selected_action.and_then(|action| {
            rows.iter()
                .position(|row| row.action == action && row.selectable())
        }) {
            self.selected = index;
            self.scroll = index.saturating_sub(selected_offset);
            return;
        }
        self.selected = 0;
        self.scroll = 0;
        self.clamp();
    }

    fn move_selection(&mut self, delta: isize) {
        let rows = self.projected();
        if rows.iter().all(|row| !row.selectable()) {
            self.selected = 0;
            return;
        }
        let direction = delta.signum();
        let mut remaining = delta.unsigned_abs();
        let mut candidate = self.selected;
        while remaining > 0 {
            let next = candidate.saturating_add_signed(direction);
            if next == candidate || next >= rows.len() {
                break;
            }
            candidate = next;
            if rows[candidate].selectable() {
                remaining = remaining.saturating_sub(1);
            }
        }
        if rows[candidate].selectable() {
            self.selected = candidate;
        }
    }
}

impl BoardApp {
    pub(super) fn open_palette(&mut self) {
        self.deactivate_range_latch();
        self.help = false;
        self.search = None;
        let context = self.capture_command_context();
        self.palette = Some(PaletteState::new(&self.settings.shortcuts, context));
    }

    pub(super) fn invalidate_palette(&mut self) {
        if self.palette.take().is_some() {
            self.layout = None;
        }
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
        let action = self
            .palette
            .as_ref()
            .and_then(|palette| palette.projected().get(index).map(|row| row.action));
        if action == Some(RowAction::Expand) {
            if let Some(palette) = &mut self.palette {
                palette.expanded = true;
                palette.selected = 0;
                palette.scroll = 0;
                palette.clamp();
            }
            self.layout = None;
            return Vec::new();
        }
        let Some(RowAction::Command {
            execution: command, ..
        }) = action
        else {
            return Vec::new();
        };
        let enabled = self
            .palette
            .as_ref()
            .and_then(|palette| palette.projected().get(index).map(ProjectedRow::selectable))
            .unwrap_or(false);
        if !enabled {
            return Vec::new();
        }
        let selection_handoff = self
            .palette
            .as_mut()
            .and_then(|palette| palette.context.take_selection_handoff());
        let merge_handoff = self
            .palette
            .as_mut()
            .and_then(|palette| palette.context.take_merge_handoff());
        self.palette = None;
        self.execute_command(
            command,
            selection_handoff,
            merge_handoff.as_deref(),
            ids,
            clock,
        )
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
            };
        };
        match *key {
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
            UiKey::Move { movement, .. } => {
                if let Some(palette) = &mut self.palette {
                    palette.query.move_cursor(movement);
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
        palette.move_selection(delta);
        palette.scroll =
            crate::ui::paging::first_visible(palette.selected, palette.scroll, visible);
        self.layout = None;
    }

    pub(super) fn ensure_palette_visible(&mut self, visible: usize) {
        let Some(palette) = &mut self.palette else {
            return;
        };
        palette.clamp();
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
