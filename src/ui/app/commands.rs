//! Keyboard commands and shared board intentions.

use crate::{
    application::{Action, Effect, InteractionMode},
    domain::{BoardOperationKind, UndoScope},
    ports::{
        editor::{CursorMovement, EditCommand},
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, UiKey, editing, pending_types::EditFlush};
use crate::ui::settings::BoardCommand;

impl BoardApp {
    pub(super) fn handle_board_key(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.insertion_focused() {
            return self.handle_insertion_key(key, ids, clock);
        }
        match key {
            UiKey::Escape if !self.selection_is_empty() || self.range_latched() => {
                self.clear_board_selection();
            }
            UiKey::SelectAll => self.select_all_thoughts(),
            UiKey::Character(character) => {
                return self.handle_board_command(character, ids, clock);
            }
            UiKey::PrimaryCharacter(character) => {
                return self.reorder_from_character(character, ids, clock);
            }
            UiKey::PrimaryShiftMove { movement } => {
                return self.reorder_from_movement(movement, ids, clock);
            }
            UiKey::Enter => return self.expand_and_enter_edit(ids, clock),
            UiKey::Move {
                movement: CursorMovement::VisualUp,
                extend_selection: true,
            } => self.extend_range_by(-1),
            UiKey::Move {
                movement: CursorMovement::VisualDown,
                extend_selection: true,
            } => self.extend_range_by(1),
            UiKey::Move {
                movement: CursorMovement::VisualUp,
                extend_selection: false,
            } if self.range_latched() => self.extend_range_by(-1),
            UiKey::Move {
                movement: CursorMovement::VisualDown,
                extend_selection: false,
            } if self.range_latched() => self.extend_range_by(1),
            UiKey::Move {
                movement: CursorMovement::VisualUp,
                extend_selection: false,
            } => self.move_focus_outside_range(-1),
            UiKey::Move {
                movement: CursorMovement::VisualDown,
                extend_selection: false,
            } => self.move_focus_outside_range(1),
            UiKey::Undo => return self.history(ids, clock, true),
            UiKey::Redo => return self.history(ids, clock, false),
            UiKey::Copy => return self.copy_thought(ids),
            UiKey::Cut => return self.cut_thought(ids, clock),
            UiKey::PasteClipboard => return self.read_clipboard(ids),
            UiKey::Duplicate => return self.duplicate(ids, clock),
            _ => {}
        }
        Vec::new()
    }

    fn handle_insertion_key(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match key {
            UiKey::Escape if !self.selection_is_empty() || self.range_latched() => {
                self.clear_board_selection();
                Vec::new()
            }
            UiKey::SelectAll => {
                self.select_all_thoughts();
                Vec::new()
            }
            UiKey::Character(character)
                if self.settings.keybindings.command(character)
                    == Some(BoardCommand::FocusDown) =>
            {
                self.confirm_insertion_creation(ids, clock)
            }
            UiKey::Character(character) => self.handle_board_command(character, ids, clock),
            UiKey::Enter => self.begin_insertion(ids, clock),
            UiKey::Escape
            | UiKey::Move {
                movement: CursorMovement::VisualUp,
                ..
            } => {
                self.move_focus(-1);
                Vec::new()
            }
            UiKey::PasteClipboard => self.read_clipboard(ids),
            UiKey::Undo => self.history(ids, clock, true),
            UiKey::Redo => self.history(ids, clock, false),
            UiKey::Move {
                movement: CursorMovement::VisualDown,
                extend_selection: false,
            } => self.confirm_insertion_creation(ids, clock),
            UiKey::Move {
                movement: CursorMovement::VisualDown,
                extend_selection: true,
            }
            | UiKey::Backspace
            | UiKey::Delete
            | UiKey::DeleteLine
            | UiKey::Copy
            | UiKey::Cut
            | UiKey::Duplicate
            | UiKey::Quit
            | UiKey::Tab
            | UiKey::BackTab
            | UiKey::PickerPrevious
            | UiKey::PickerNext
            | UiKey::PrimaryCharacter(_)
            | UiKey::PrimaryShiftMove { .. }
            | UiKey::EditNavigation { .. }
            | UiKey::Submit
            | UiKey::SubmitKeep
            | UiKey::Move { .. } => Vec::new(),
        }
    }

    pub(super) fn reset_insertion_confirmation(&mut self, input: &crate::ui::UiInput) {
        let continues = self.insertion_focused()
            && match input {
                crate::ui::UiInput::Key(UiKey::Move {
                    movement: CursorMovement::VisualDown,
                    extend_selection: false,
                }) => true,
                crate::ui::UiInput::Key(UiKey::Character(character)) => {
                    self.settings.keybindings.command(*character) == Some(BoardCommand::FocusDown)
                }
                _ => false,
            };
        if !continues {
            self.insertion_confirmation = super::InsertionConfirmation::Idle;
        }
    }

    fn confirm_insertion_creation(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.insertion_confirmation == super::InsertionConfirmation::Armed {
            self.begin_insertion(ids, clock)
        } else {
            self.insertion_confirmation = super::InsertionConfirmation::Armed;
            Vec::new()
        }
    }

    fn handle_board_command(
        &mut self,
        character: char,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.range_latched() {
            match self.settings.keybindings.command(character) {
                Some(BoardCommand::FocusUp) => {
                    self.extend_range_by(-1);
                    return Vec::new();
                }
                Some(BoardCommand::FocusDown) => {
                    self.extend_range_by(1);
                    return Vec::new();
                }
                _ => {}
            }
        }
        match self.settings.keybindings.command(character) {
            Some(BoardCommand::New) => self.begin_insertion(ids, clock),
            Some(BoardCommand::Edit) => self.expand_and_enter_edit(ids, clock),
            Some(BoardCommand::Delete) => self.delete(ids, clock),
            Some(BoardCommand::Copy) => self.copy_thought(ids),
            Some(BoardCommand::Cut) => self.cut_thought(ids, clock),
            Some(BoardCommand::SubmitRemove) => self.begin_delivery(
                crate::ports::agent::SubmissionDisposition::RemoveAfterSuccess,
                ids,
                clock,
            ),
            Some(BoardCommand::SubmitKeep) => {
                self.begin_delivery(crate::ports::agent::SubmissionDisposition::Keep, ids, clock)
            }
            Some(BoardCommand::Undo) => self.history(ids, clock, true),
            Some(BoardCommand::FocusUp) => {
                self.move_focus_outside_range(-1);
                Vec::new()
            }
            Some(BoardCommand::FocusDown) => {
                self.move_focus_outside_range(1);
                Vec::new()
            }
            Some(BoardCommand::RangeUp) => {
                self.extend_range_by(-1);
                Vec::new()
            }
            Some(BoardCommand::RangeDown) => {
                self.extend_range_by(1);
                Vec::new()
            }
            Some(BoardCommand::Collapse) => self.collapse(ids, clock),
            Some(BoardCommand::Select) => {
                self.toggle_selection();
                Vec::new()
            }
            Some(BoardCommand::SelectAll) => {
                self.select_all_thoughts();
                Vec::new()
            }
            Some(BoardCommand::RangeSelect) => {
                self.activate_range_latch();
                Vec::new()
            }
            Some(BoardCommand::Search) => {
                self.open_search();
                Vec::new()
            }
            Some(BoardCommand::Commands) => {
                self.open_palette();
                Vec::new()
            }
            Some(BoardCommand::Help) => self.toggle_help(),
            Some(BoardCommand::Quit) => {
                self.request_quit();
                Vec::new()
            }
            Some(BoardCommand::ScreenshotInbox) => self.toggle_screenshot_inbox(ids, clock),
            None => Vec::new(),
        }
    }

    pub(super) fn handle_edit_key(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(key) = editing::normalize_edit_key(key) else {
            return Vec::new();
        };
        if let Some(effects) = self.handle_edit_effect(key, ids, clock) {
            return effects;
        }
        if let UiKey::Move {
            movement,
            extend_selection,
        } = key
            && self.leave_selected_fold(movement, extend_selection)
        {
            return self.flush_pending_edit(ids, clock);
        }
        if matches!(key, UiKey::Enter) && self.should_insert_smart_newline() {
            return self.insert_newline(true, ids, clock);
        }
        if matches!(key, UiKey::Tab | UiKey::BackTab) {
            return self.apply_indentation(matches!(key, UiKey::BackTab), ids, clock);
        }
        let adjacent_fold = match key {
            UiKey::Backspace => self.delete_adjacent_fold(true),
            UiKey::Delete => self.delete_adjacent_fold(false),
            _ => false,
        };
        let Some((command, boundary)) = editing::command_for_key(key, adjacent_fold) else {
            return Vec::new();
        };
        let movement = match &command {
            EditCommand::Move {
                movement,
                extend_selection,
            } => Some((*movement, *extend_selection)),
            _ => None,
        };
        let before_movement = movement.and_then(|_| self.editor_snapshot());
        let mut effects = if boundary {
            self.flush_pending_edit(ids, clock)
        } else {
            Vec::new()
        };
        self.apply_edit(command);
        if let Some((movement, extend_selection)) = movement {
            self.normalize_fold_cursor(movement, extend_selection);
            effects.extend(self.finish_boundary_navigation(
                movement,
                extend_selection,
                before_movement.as_ref(),
                ids,
                clock,
            ));
        }
        if matches!(key, UiKey::DeleteLine) {
            effects.extend(self.flush_pending_edit(ids, clock));
        }
        effects
    }

    fn handle_edit_effect(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Option<Vec<Effect>> {
        if matches!(key, UiKey::Enter) && self.expand_fold_at_cursor() {
            return Some(Vec::new());
        }
        match key {
            UiKey::Escape => Some(self.finish_edit(ids, clock)),
            UiKey::Submit => Some(self.begin_edit_delivery(
                crate::ports::agent::SubmissionDisposition::RemoveAfterSuccess,
                ids,
                clock,
            )),
            UiKey::SubmitKeep => Some(self.begin_edit_delivery(
                crate::ports::agent::SubmissionDisposition::Keep,
                ids,
                clock,
            )),
            UiKey::Undo => Some(self.history(ids, clock, true)),
            UiKey::Redo => Some(self.history(ids, clock, false)),
            UiKey::Copy => {
                let mut effects = self.flush_pending_edit(ids, clock);
                effects.extend(self.copy_selection(ids));
                Some(effects)
            }
            UiKey::Cut => {
                let mut effects = self.flush_pending_edit(ids, clock);
                effects.extend(self.cut_selection(ids));
                Some(effects)
            }
            UiKey::PasteClipboard => {
                let mut effects = self.flush_pending_edit(ids, clock);
                effects.extend(self.read_clipboard(ids));
                Some(effects)
            }
            _ => None,
        }
    }

    pub(super) fn finish_edit(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.edit_boundary = None;
        let thought_id = self.active_thought_id();
        self.capture_palette_selection_handoff();
        let effects = match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        if let Some(thought_id) = thought_id {
            self.clear_expanded_folds(thought_id);
        }
        let _effects = self.reduce(Action::ExitEdit);
        self.editor = None;
        effects
    }

    pub(super) fn delete(&mut self, ids: &mut impl IdGenerator, clock: &impl Clock) -> Vec<Effect> {
        let thought_ids = self.action_thought_ids();
        if thought_ids.is_empty() {
            return Vec::new();
        }
        if thought_ids.iter().any(|id| self.submission_locked(*id)) {
            self.set_warning("selected thought has a submission in progress");
            return Vec::new();
        }
        let effects = self.reduce_with_empty_transition(
            Action::DeleteThoughts {
                operation_id: ids.operation_id(),
                thought_ids,
                kind: BoardOperationKind::Delete,
                at: clock.now(),
            },
            crate::application::EmptyBoardTransition::ComposeAfterLocalRemoval,
        );
        self.clear_board_selection();
        self.sync_empty_insertion_focus();
        effects
    }

    pub(super) fn history(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
        undo: bool,
    ) -> Vec<Effect> {
        let mut effects = match self.flush_edit_boundary(ids, clock) {
            EditFlush::Complete(effects) => effects,
            EditFlush::Blocked(effects) => return effects,
        };
        let scope = match self.state.mode {
            InteractionMode::Board => UndoScope::Board,
            InteractionMode::Compose => return effects,
            InteractionMode::Edit { thought_id } => UndoScope::Editor { thought_id },
        };
        let action = if undo {
            Action::Undo {
                operation_id: ids.operation_id(),
                scope,
                at: clock.now(),
            }
        } else {
            Action::Redo {
                operation_id: ids.operation_id(),
                scope,
                at: clock.now(),
            }
        };
        effects.extend(self.reduce_with_empty_transition(
            action,
            crate::application::EmptyBoardTransition::ComposeAfterLocalRemoval,
        ));
        self.reload_editor();
        self.sync_empty_insertion_focus();
        effects
    }

    pub(super) fn move_focus(&mut self, delta: isize) {
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        self.insertion_confirmation = super::InsertionConfirmation::Idle;
        let live = self.state.board.live_thoughts();
        if live.is_empty() {
            self.insertion_focus = super::InsertionFocus::Active;
            self.layout = None;
            return;
        }
        if self.insertion_focus == super::InsertionFocus::Active {
            if delta < 0 {
                self.insertion_focus = super::InsertionFocus::Inactive;
                let _effects = self.reduce(Action::FocusThought(Some(live[live.len() - 1].id)));
            }
            return;
        }
        let current = self
            .state
            .focused_thought
            .and_then(|id| live.iter().position(|thought| thought.id == id))
            .unwrap_or(0);
        if delta > 0 && current == live.len() - 1 {
            self.insertion_focus = super::InsertionFocus::Active;
            self.layout = None;
            return;
        }
        let target = current.saturating_add_signed(delta).min(live.len() - 1);
        let _effects = self.reduce(Action::FocusThought(Some(live[target].id)));
    }

    pub(super) fn sync_empty_insertion_focus(&mut self) {
        if self.state.board.live_thoughts().is_empty() {
            self.insertion_focus = super::InsertionFocus::Active;
            self.layout = None;
        } else if self.state.focused_thought.is_some() {
            self.insertion_focus = super::InsertionFocus::Inactive;
        }
    }
}
