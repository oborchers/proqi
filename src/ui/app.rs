//! Terminal-independent board interaction state.

use crate::{
    adapters::editor::RopeEditor,
    application::{Action, AppState, Effect, FailureCode, InteractionMode, reduce},
    domain::{BoardOperationKind, OperationSequence, ThoughtId, UndoScope},
    ports::{
        editor::{CursorMovement, EditCommand, Editor, EditorSnapshot, TextViewport},
        environment::{Clock, IdGenerator},
    },
};

/// Normalized keys accepted by the board UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiKey {
    /// Request a clean application exit from any mode.
    Quit,
    /// Insert one Unicode scalar value.
    Character(char),
    /// Insert a line break or enter the focused thought.
    Enter,
    /// Return from edit mode.
    Escape,
    /// Delete the preceding grapheme.
    Backspace,
    /// Delete the following grapheme.
    Delete,
    /// Move upward.
    Up,
    /// Move downward.
    Down,
    /// Move left.
    Left,
    /// Move right.
    Right,
    /// Move to the logical line start.
    Home,
    /// Move to the logical line end.
    End,
    /// Select the complete thought.
    SelectAll,
    /// Delete the current logical line.
    DeleteLine,
    /// Undo in the active history scope.
    Undo,
    /// Redo in the active history scope.
    Redo,
}

/// Input translated from a concrete terminal backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiInput {
    /// One normalized key command.
    Key(UiKey),
    /// One complete bracketed or clipboard paste.
    Paste(String),
    /// Latest terminal cell dimensions.
    Resize {
        /// Latest reported terminal width.
        width: u16,
        /// Latest reported terminal height.
        height: u16,
    },
}

/// Mutable UI state around the pure application reducer.
pub struct BoardApp {
    /// Reducer-owned application state rendered by the board.
    pub state: AppState,
    editor: Option<(ThoughtId, RopeEditor)>,
    /// Whether the user requested a clean exit.
    pub quit: bool,
    /// Whether contextual help is visible.
    pub help: bool,
    /// Transient human-readable status.
    pub status: Option<String>,
    viewport: TextViewport,
}

impl BoardApp {
    /// Construct a board around rehydrated application state.
    #[must_use]
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            editor: None,
            quit: false,
            help: false,
            status: None,
            viewport: TextViewport::default(),
        }
    }

    /// Apply normalized input and return ordered external effects.
    pub fn handle(
        &mut self,
        input: UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.status = None;
        if input == UiInput::Key(UiKey::Quit) {
            self.quit = true;
            return Vec::new();
        }
        match input {
            UiInput::Resize { .. } => Vec::new(),
            UiInput::Paste(content) => self.paste(content, ids, clock),
            UiInput::Key(key) => match self.state.mode {
                InteractionMode::Board => self.handle_board_key(key, ids, clock),
                InteractionMode::Edit { .. } => self.handle_edit_key(key, ids, clock),
            },
        }
    }

    /// Return the active editor snapshot, if edit mode is active.
    #[must_use]
    pub fn editor_snapshot(&self) -> Option<EditorSnapshot> {
        self.editor.as_ref().map(|(_, editor)| editor.snapshot())
    }

    /// Prepare current frame geometry without changing the logical cursor.
    pub fn prepare_layout(&mut self, viewport: TextViewport) {
        self.viewport = viewport;
        if let Some((_, editor)) = &mut self.editor {
            editor.set_viewport(viewport);
        }
    }

    /// Rebuild the editor adapter when reducer state changes externally.
    pub fn sync_editor_from_state(&mut self) {
        let InteractionMode::Edit { thought_id } = self.state.mode else {
            self.editor = None;
            return;
        };
        let Some(thought) = self.state.board.thought(thought_id) else {
            self.editor = None;
            return;
        };
        if self
            .editor
            .as_ref()
            .is_none_or(|(current, _)| *current != thought_id)
        {
            let mut editor = RopeEditor::new(&thought.content);
            editor.set_viewport(self.viewport);
            let _outcome = editor.apply(EditCommand::Move {
                movement: CursorMovement::DocumentEnd,
                extend_selection: false,
            });
            self.editor = Some((thought_id, editor));
        }
    }

    /// Apply one ordered persistence acknowledgement to the reducer state.
    pub fn acknowledge_persistence(&mut self, sequence: OperationSequence, succeeded: bool) {
        let action = if succeeded {
            Action::PersistenceCommitted(sequence)
        } else {
            Action::PersistenceFailed {
                sequence,
                code: FailureCode::StorageFailed,
            }
        };
        let _effects = self.reduce(action);
    }

    fn handle_board_key(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match key {
            UiKey::Character('q') => self.quit = true,
            UiKey::Character('n') => return self.create(String::new(), ids, clock),
            UiKey::Enter | UiKey::Character('e') => self.enter_edit(),
            UiKey::Up | UiKey::Character('k') => self.move_focus(-1),
            UiKey::Down | UiKey::Character('j') => self.move_focus(1),
            UiKey::Character('d') => return self.delete(ids, clock),
            UiKey::Character('u') | UiKey::Undo => return self.history(ids, clock, true),
            UiKey::Redo => return self.history(ids, clock, false),
            UiKey::Character('J') => return self.reorder(ids, clock, 1),
            UiKey::Character('K') => return self.reorder(ids, clock, -1),
            UiKey::Character(' ') => return self.collapse(ids, clock),
            UiKey::Character('?') => self.help = !self.help,
            _ => {}
        }
        Vec::new()
    }

    fn handle_edit_key(
        &mut self,
        key: UiKey,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match key {
            UiKey::Escape => {
                let _effects = self.reduce(Action::ExitEdit);
                self.editor = None;
                return Vec::new();
            }
            UiKey::Undo => return self.history(ids, clock, true),
            UiKey::Redo => return self.history(ids, clock, false),
            _ => {}
        }
        let command = match key {
            UiKey::Character(character) => EditCommand::InsertChar(character),
            UiKey::Enter => EditCommand::InsertNewline,
            UiKey::Backspace => EditCommand::DeleteBack,
            UiKey::Delete => EditCommand::DeleteForward,
            UiKey::Left => movement(CursorMovement::GraphemeBack),
            UiKey::Right => movement(CursorMovement::GraphemeForward),
            UiKey::Up => movement(CursorMovement::VisualUp),
            UiKey::Down => movement(CursorMovement::VisualDown),
            UiKey::Home => movement(CursorMovement::LineStart),
            UiKey::End => movement(CursorMovement::LineEnd),
            UiKey::SelectAll => EditCommand::SelectAll,
            UiKey::DeleteLine => EditCommand::DeleteLogicalLine,
            UiKey::Escape | UiKey::Undo | UiKey::Redo | UiKey::Quit => return Vec::new(),
        };
        self.apply_edit(command, ids, clock)
    }

    fn paste(
        &mut self,
        content: String,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if matches!(self.state.mode, InteractionMode::Board) {
            self.create(content, ids, clock)
        } else {
            self.apply_edit(EditCommand::Paste(content), ids, clock)
        }
    }

    fn create(
        &mut self,
        content: String,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let effects = self.reduce(Action::CreateThought {
            thought_id: ids.thought_id(),
            operation_id: ids.operation_id(),
            content,
            insertion_index: None,
            at: clock.now(),
        });
        self.sync_editor_from_state();
        effects
    }

    fn enter_edit(&mut self) {
        if let Some(thought_id) = self.state.focused_thought {
            let _effects = self.reduce(Action::EnterEdit(thought_id));
            self.sync_editor_from_state();
        }
    }

    fn apply_edit(
        &mut self,
        command: EditCommand,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let edit = self.editor.as_mut().and_then(|(thought_id, editor)| {
            let before = editor.snapshot();
            let outcome = editor.apply(command);
            outcome
                .content_changed
                .then_some((*thought_id, before, outcome.snapshot))
        });
        let Some((thought_id, before, after)) = edit else {
            return Vec::new();
        };
        self.reduce(Action::EditThought {
            thought_id,
            revision_id: ids.revision_id(),
            before_content: before.content,
            after_content: after.content,
            before_cursor: before.cursor,
            after_cursor: after.cursor,
            at: clock.now(),
        })
    }

    fn delete(&mut self, ids: &mut impl IdGenerator, clock: &impl Clock) -> Vec<Effect> {
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

    fn history(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
        undo: bool,
    ) -> Vec<Effect> {
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
        let effects = self.reduce(action);
        self.reload_editor();
        effects
    }

    fn move_focus(&mut self, delta: isize) {
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

    fn reorder(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
        delta: isize,
    ) -> Vec<Effect> {
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

    fn collapse(&mut self, ids: &mut impl IdGenerator, clock: &impl Clock) -> Vec<Effect> {
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        let Some(thought) = self.state.board.thought(thought_id) else {
            return Vec::new();
        };
        self.reduce(Action::SetCollapsed {
            operation_id: ids.operation_id(),
            thought_id,
            collapsed: !thought.collapsed,
            at: clock.now(),
        })
    }

    fn reload_editor(&mut self) {
        self.editor = None;
        self.sync_editor_from_state();
    }

    fn reduce(&mut self, action: Action) -> Vec<Effect> {
        match reduce(&mut self.state, action) {
            Ok(effects) => effects,
            Err(error) => {
                self.status = Some(error.to_string());
                Vec::new()
            }
        }
    }
}

const fn movement(movement: CursorMovement) -> EditCommand {
    EditCommand::Move {
        movement,
        extend_selection: false,
    }
}
