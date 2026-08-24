//! Keyboard commands and shared board intentions.

use crate::{
    application::{Action, Effect, InteractionMode},
    domain::{BoardOperationKind, UndoScope},
    ports::{
        editor::{CursorMovement, EditCommand},
        environment::{Clock, IdGenerator},
    },
};

use super::{BoardApp, PastePayload, UiKey};
use crate::ui::settings::BoardCommand;

impl BoardApp {
    pub(super) fn handle_board_key(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match key {
            UiKey::Character(character) => {
                return self.handle_board_command(character, ids, clock);
            }
            UiKey::Enter => self.enter_edit(),
            UiKey::Move {
                movement: CursorMovement::VisualUp,
                ..
            } => self.move_focus(-1),
            UiKey::Move {
                movement: CursorMovement::VisualDown,
                ..
            } => self.move_focus(1),
            UiKey::Undo => return self.history(ids, clock, true),
            UiKey::Redo => return self.history(ids, clock, false),
            UiKey::Copy => return self.copy_thought(ids),
            UiKey::Cut => return self.cut_thought(ids, clock),
            UiKey::PasteClipboard => return self.read_clipboard(ids),
            _ => {}
        }
        Vec::new()
    }

    fn handle_board_command(
        &mut self,
        character: char,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match self.settings.keybindings.command(character) {
            Some(BoardCommand::New) => self.create(PastePayload::text(String::new()), ids, clock),
            Some(BoardCommand::Edit) => {
                self.enter_edit();
                Vec::new()
            }
            Some(BoardCommand::Delete) => self.delete(ids, clock),
            Some(BoardCommand::Copy) => self.copy_thought(ids),
            Some(BoardCommand::Cut) => self.cut_thought(ids, clock),
            Some(BoardCommand::Submit) => self.begin_submission(false, ids, clock),
            Some(BoardCommand::SubmitRemove) => self.begin_submission(true, ids, clock),
            Some(BoardCommand::Undo) => self.history(ids, clock, true),
            Some(BoardCommand::FocusUp) => {
                self.move_focus(-1);
                Vec::new()
            }
            Some(BoardCommand::FocusDown) => {
                self.move_focus(1);
                Vec::new()
            }
            Some(BoardCommand::MoveUp) => self.reorder(ids, clock, -1),
            Some(BoardCommand::MoveDown) => self.reorder(ids, clock, 1),
            Some(BoardCommand::Collapse) => self.collapse(ids, clock),
            Some(BoardCommand::Search) => {
                self.open_search();
                Vec::new()
            }
            Some(BoardCommand::Commands) => {
                self.open_palette();
                Vec::new()
            }
            Some(BoardCommand::Help) => {
                self.help = !self.help;
                Vec::new()
            }
            Some(BoardCommand::Quit) => {
                self.request_quit();
                Vec::new()
            }
            None => Vec::new(),
        }
    }

    pub(super) fn handle_edit_key(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match key {
            UiKey::Escape => {
                if self.has_draft() {
                    self.discard_draft();
                    return Vec::new();
                }
                let effects = self.flush_pending_edit(ids, clock);
                let _effects = self.reduce(Action::ExitEdit);
                self.editor = None;
                return effects;
            }
            UiKey::Undo => return self.history(ids, clock, true),
            UiKey::Redo => return self.history(ids, clock, false),
            UiKey::Copy => {
                let mut effects = self.flush_pending_edit(ids, clock);
                effects.extend(self.copy_selection(ids));
                return effects;
            }
            UiKey::Cut => {
                let mut effects = self.flush_pending_edit(ids, clock);
                effects.extend(self.cut_selection(ids));
                return effects;
            }
            UiKey::PasteClipboard => {
                let mut effects = self.flush_pending_edit(ids, clock);
                effects.extend(self.read_clipboard(ids));
                return effects;
            }
            _ => {}
        }
        let (command, boundary) = match key {
            UiKey::Character(character) => (EditCommand::InsertChar(character), false),
            UiKey::Enter => (EditCommand::InsertNewline, false),
            UiKey::Backspace => (EditCommand::DeleteBack, false),
            UiKey::Delete => (EditCommand::DeleteForward, false),
            UiKey::Move {
                movement,
                extend_selection,
            } => (
                EditCommand::Move {
                    movement,
                    extend_selection,
                },
                true,
            ),
            UiKey::SelectAll => (EditCommand::SelectAll, true),
            UiKey::DeleteLine => (EditCommand::DeleteLogicalLine, true),
            UiKey::Escape
            | UiKey::Undo
            | UiKey::Redo
            | UiKey::Quit
            | UiKey::Copy
            | UiKey::Cut
            | UiKey::PasteClipboard => return Vec::new(),
        };
        let mut effects = if boundary {
            self.flush_pending_edit(ids, clock)
        } else {
            Vec::new()
        };
        self.apply_edit(command);
        if self.has_draft() {
            effects.extend(self.persist_draft(ids, clock));
        }
        if matches!(key, UiKey::DeleteLine) {
            effects.extend(self.flush_pending_edit(ids, clock));
        }
        effects
    }

    pub(super) fn paste_payload(
        &mut self,
        payload: PastePayload,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if self.has_draft() {
            self.set_draft_annotations(payload.annotations.clone());
            self.apply_annotated_edit(EditCommand::Paste(payload.content), &payload.annotations);
            self.persist_draft(ids, clock)
        } else if matches!(self.state.mode, InteractionMode::Board) {
            if payload.content.is_empty() {
                return Vec::new();
            }
            self.create(payload, ids, clock)
        } else {
            let mut effects = self.flush_pending_edit(ids, clock);
            self.apply_annotated_edit(EditCommand::Paste(payload.content), &payload.annotations);
            effects.extend(self.flush_pending_edit(ids, clock));
            effects
        }
    }

    pub(super) fn delete(&mut self, ids: &mut impl IdGenerator, clock: &impl Clock) -> Vec<Effect> {
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        self.reduce(Action::DeleteThought {
            operation_id: ids.operation_id(),
            thought_id,
            kind: BoardOperationKind::Delete,
            at: clock.now(),
        })
    }

    pub(super) fn history(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
        undo: bool,
    ) -> Vec<Effect> {
        let mut effects = self.flush_pending_edit(ids, clock);
        let scope = match self.state.mode {
            InteractionMode::Board => UndoScope::Board,
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
        effects.extend(self.reduce(action));
        self.reload_editor();
        effects
    }

    pub(super) fn move_focus(&mut self, delta: isize) {
        self.manual_board_scroll = false;
        let live = self.state.board.live_thoughts();
        if live.is_empty() {
            return;
        }
        let current = self
            .state
            .focused_thought
            .and_then(|id| live.iter().position(|thought| thought.id == id))
            .unwrap_or(0);
        let target = current.saturating_add_signed(delta).min(live.len() - 1);
        let _effects = self.reduce(Action::FocusThought(Some(live[target].id)));
    }

    pub(super) fn reorder(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
        delta: isize,
    ) -> Vec<Effect> {
        self.manual_board_scroll = false;
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        let live = self.state.board.live_thoughts();
        let Some(current) = live.iter().position(|thought| thought.id == thought_id) else {
            return Vec::new();
        };
        let target = current
            .saturating_add_signed(delta)
            .min(live.len().saturating_sub(1));
        self.reduce(Action::MoveThought {
            operation_id: ids.operation_id(),
            thought_id,
            to: target,
            at: clock.now(),
        })
    }

    pub(super) fn collapse(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        let Some(thought) = self.state.board.thought(thought_id) else {
            return Vec::new();
        };
        let collapsed = thought.collapsed;
        let capped = self
            .layout
            .as_ref()
            .and_then(|layout| layout.thought(thought_id))
            .is_some_and(|layout| layout.hidden_rows > 0);
        if !collapsed && self.expanded.remove(&thought_id) {
            return Vec::new();
        }
        if !collapsed && capped {
            self.expanded.insert(thought_id);
            return Vec::new();
        }
        self.expanded.remove(&thought_id);
        self.reduce(Action::SetCollapsed {
            operation_id: ids.operation_id(),
            thought_id,
            collapsed: !collapsed,
            at: clock.now(),
        })
    }
}
