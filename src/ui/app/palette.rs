//! Searchable command discovery and execution.

mod binding;
mod dispatch;
mod editor;
mod input;
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

#[derive(Clone, Copy)]
struct PaletteHistoryContext {
    can_undo: bool,
    can_redo: bool,
}

impl PaletteHistoryContext {
    const EMPTY: Self = Self {
        can_undo: false,
        can_redo: false,
    };
}

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
    rendered_scroll: usize,
    rendered_interactivity: Option<Vec<bool>>,
}

impl PaletteState {
    fn new(registry: &crate::ui::ShortcutRegistry, context: CommandContext) -> Self {
        let commands = registry
            .commands()
            .into_iter()
            .map(|(action, metadata, execution)| {
                let shortcut = context
                    .command_binding_context(action, metadata)
                    .map(|binding_context| {
                        registry.compact_help_label(binding_context, &[metadata.shortcut_owner])
                    })
                    .unwrap_or_default();
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
            rendered_scroll: 0,
            rendered_interactivity: None,
        };
        state.clamp();
        state
    }

    pub(super) const fn query_cursor(&self) -> usize {
        self.query.cursor()
    }

    pub(super) const fn query_selection(&self) -> Option<super::query::QuerySelection> {
        self.query.selection()
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
        let history = PaletteHistoryContext {
            can_undo: self.query.can_undo(),
            can_redo: self.query.can_redo(),
        };
        projection::rows(
            &self.commands,
            &self.context,
            self.query.text(),
            history,
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
        self.invalidate_rendered_geometry();
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

    fn record_rendered_geometry(&mut self, interactivity: Vec<bool>) {
        self.rendered_scroll = self.scroll;
        self.rendered_interactivity = Some(interactivity);
    }

    fn invalidate_rendered_geometry(&mut self) {
        if self.rendered_interactivity.is_some() {
            self.rendered_interactivity = Some(Vec::new());
        }
    }

    fn selected_has_rendered_geometry(&self) -> bool {
        let Some(interactivity) = &self.rendered_interactivity else {
            return true;
        };
        let visible = self.selected.saturating_sub(self.rendered_scroll);
        interactivity.get(visible).copied().unwrap_or(false)
    }

    fn execution_at(&self, index: usize) -> Option<CommandExecution> {
        self.projected().get(index).and_then(|row| {
            if !row.selectable() {
                return None;
            }
            match row.action {
                RowAction::Command { execution, .. } => Some(execution),
                RowAction::Expand | RowAction::None => None,
            }
        })
    }
}

impl BoardApp {
    pub(super) fn open_palette(&mut self) {
        self.deactivate_range_latch();
        self.help = false;
        self.search = None;
        let context = self.capture_command_context();
        self.palette = Some(PaletteState::new(&self.settings.shortcuts, context));
        self.layout = None;
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
                palette.invalidate_rendered_geometry();
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
        if let Some(undo) = palette_query_history(Some(command)) {
            return self.update_palette_query(|query| move_query_history(query, undo));
        }
        let selection_handoff = self
            .palette
            .as_mut()
            .and_then(|palette| palette.context.take_selection_handoff());
        let merge_handoff = self
            .palette
            .as_mut()
            .and_then(|palette| palette.context.take_merge_handoff());
        let retain_palette = command_requests_quit(Some(command)) && self.screenshot_retry_ready();
        if !retain_palette {
            self.palette = None;
        }
        let effects = self.execute_command(
            command,
            selection_handoff,
            merge_handoff.as_deref(),
            ids,
            clock,
        );
        if self.quit {
            self.palette = None;
        }
        effects
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
                self.new_thought(super::creation::NewThoughtPlacement::Contextual, ids, clock)
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
            BoardCommand::Quit => self.request_global_quit(ids, clock),
        }
    }
}

fn command_requests_quit(command: Option<CommandExecution>) -> bool {
    matches!(
        command,
        Some(CommandExecution::Board(
            crate::ui::shortcut_registry::PaletteBoardCommand::Quit
        ))
    )
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
