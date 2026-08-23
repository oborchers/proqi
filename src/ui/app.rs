//! Terminal-independent board interaction state.

mod agent;
mod clipboard;
mod commands;
mod control;
mod palette;
mod pointer;
mod recovery;

use std::collections::{BTreeMap, BTreeSet};

use ratatui_core::layout::Rect;

use crate::{
    application::{
        Action, AppState, ClipboardIntent, DurabilityState, Effect, FailureCode, InteractionMode,
        reduce,
    },
    domain::{OperationSequence, RequestId, SubmissionId, ThoughtId},
    ports::{
        agent::{AgentTarget, SubmissionRequest},
        editor::{
            CursorMovement, EditCommand, Editor, EditorFactory, EditorSnapshot, TextViewport,
        },
        environment::{Clock, IdGenerator},
    },
};

use super::{HitTarget, LayoutSnapshot, UiSettings, compute_layout};

/// Mouse button after terminal-backend normalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerButton {
    /// Primary button used for all required interactions.
    Left,
    /// Middle button, retained for portable event normalization.
    Middle,
    /// Secondary button, never required by Proqi.
    Right,
}

/// Semantic pointer event kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerKind {
    /// Button pressed.
    Down(PointerButton),
    /// Button released.
    Up(PointerButton),
    /// Pointer moved while a button is held.
    Drag(PointerButton),
    /// Pointer moved without a button.
    Move,
    /// Scroll toward earlier content.
    ScrollUp,
    /// Scroll toward later content.
    ScrollDown,
}

/// Terminal-cell pointer location and semantic event kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerInput {
    /// Zero-based terminal column.
    pub column: u16,
    /// Zero-based terminal row.
    pub row: u16,
    /// Normalized pointer event.
    pub kind: PointerKind,
}

struct PendingEditorClipboard {
    intent: ClipboardIntent,
    before: EditorSnapshot,
}

struct PendingSubmission {
    request: SubmissionRequest,
    thought_id: ThoughtId,
    operation_id: crate::domain::OperationId,
    at: crate::domain::Timestamp,
    remove: bool,
}

#[derive(Clone, Copy)]
struct SubmissionMode {
    remove: bool,
}

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
    /// Move logically or visually, optionally extending selection.
    Move {
        /// Backend-independent cursor intention.
        movement: CursorMovement,
        /// Whether to extend the active selection.
        extend_selection: bool,
    },
    /// Select the complete thought.
    SelectAll,
    /// Delete the current logical line.
    DeleteLine,
    /// Undo in the active history scope.
    Undo,
    /// Redo in the active history scope.
    Redo,
    /// Copy the active thought or editor selection.
    Copy,
    /// Cut the active thought or editor selection after clipboard success.
    Cut,
    /// Read and paste the native clipboard.
    PasteClipboard,
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
    /// One normalized mouse or trackpad event.
    Pointer(PointerInput),
}

/// Mutable UI state around the pure application reducer.
pub struct BoardApp {
    /// Reducer-owned application state rendered by the board.
    pub state: AppState,
    editor: Option<(ThoughtId, Box<dyn Editor>)>,
    editor_factory: Box<dyn EditorFactory>,
    /// Whether the user requested a clean exit.
    pub quit: bool,
    /// Whether contextual help is visible.
    pub help: bool,
    /// Transient human-readable status.
    pub status: Option<String>,
    viewport: TextViewport,
    first_visible: usize,
    manual_board_scroll: bool,
    layout: Option<LayoutSnapshot>,
    dragged_thought: Option<ThoughtId>,
    drag_target: Option<usize>,
    hovered: Option<HitTarget>,
    palette: Option<palette::PaletteState>,
    settings: UiSettings,
    expanded: BTreeSet<ThoughtId>,
    pending_editor_clipboard: BTreeMap<RequestId, PendingEditorClipboard>,
    pending_clipboard_reads: BTreeSet<RequestId>,
    pending_recovery_exports: BTreeSet<RequestId>,
    recovery_exported_for: Option<OperationSequence>,
    agent_targets: Vec<AgentTarget>,
    submission_mode: Option<SubmissionMode>,
    pending_submissions: BTreeMap<SubmissionId, PendingSubmission>,
}

impl BoardApp {
    /// Construct a board around rehydrated application state.
    #[must_use]
    pub fn new(state: AppState, editor_factory: impl EditorFactory + 'static) -> Self {
        Self::with_settings(state, UiSettings::default(), editor_factory)
    }

    /// Construct a board with validated user settings.
    #[must_use]
    pub fn with_settings(
        state: AppState,
        settings: UiSettings,
        editor_factory: impl EditorFactory + 'static,
    ) -> Self {
        Self {
            state,
            editor: None,
            editor_factory: Box::new(editor_factory),
            quit: false,
            help: false,
            status: None,
            viewport: TextViewport::default(),
            first_visible: 0,
            manual_board_scroll: false,
            layout: None,
            dragged_thought: None,
            drag_target: None,
            hovered: None,
            palette: None,
            settings,
            expanded: BTreeSet::new(),
            pending_editor_clipboard: BTreeMap::new(),
            pending_clipboard_reads: BTreeSet::new(),
            pending_recovery_exports: BTreeSet::new(),
            recovery_exported_for: None,
            agent_targets: Vec::new(),
            submission_mode: None,
            pending_submissions: BTreeMap::new(),
        }
    }

    /// Apply normalized input and return ordered external effects.
    pub fn handle(
        &mut self,
        input: UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if input == UiInput::Key(UiKey::Quit) {
            self.request_quit();
            return Vec::new();
        }
        if !matches!(input, UiInput::Resize { .. }) {
            self.status = None;
        }
        if self.palette.is_some() {
            return self.handle_palette_input(&input, ids, clock);
        }
        if self.submission_mode.is_some()
            && let Some(effects) = self.handle_submission_input(&input, ids, clock)
        {
            return effects;
        }
        if matches!(self.state.durability, DurabilityState::Failed { .. }) {
            match input {
                UiInput::Key(UiKey::Character('r')) => return self.retry_persistence(),
                UiInput::Key(UiKey::Character('w')) => return self.export_recovery(ids, clock),
                _ => {}
            }
        }
        match input {
            UiInput::Resize { .. } => {
                self.layout = None;
                self.hovered = None;
                Vec::new()
            }
            UiInput::Pointer(pointer) => self.handle_pointer(pointer, ids, clock),
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

    /// Current hover target resolved from the latest rendered layout.
    #[must_use]
    pub const fn hovered(&self) -> Option<HitTarget> {
        self.hovered
    }

    /// Filtered command labels and current selection for rendering.
    #[must_use]
    pub fn palette_view(&self) -> Option<(String, Vec<&'static str>, usize)> {
        self.palette.as_ref().map(palette::PaletteState::view)
    }

    /// Active board bindings used by hints and command translation.
    #[must_use]
    pub const fn keybindings(&self) -> &crate::ui::KeyBindings {
        &self.settings.keybindings
    }

    /// Currently verified submission targets, empty when the enhancement is unavailable.
    #[must_use]
    pub fn agent_targets(&self) -> &[AgentTarget] {
        &self.agent_targets
    }

    /// Whether directional submission targeting is active and removes on acceptance.
    #[must_use]
    pub fn submission_mode(&self) -> Option<bool> {
        self.submission_mode.map(|mode| mode.remove)
    }

    /// Prepare current frame geometry without changing the logical cursor.
    pub fn prepare_layout(&mut self, viewport: TextViewport) {
        self.viewport = viewport;
        if let Some((_, editor)) = &mut self.editor {
            editor.set_viewport(viewport);
        }
    }

    /// Recompute one authoritative frame layout and reflow the active editor.
    pub fn prepare_frame(&mut self, area: Rect) -> LayoutSnapshot {
        let mut layout_state = self.state.clone();
        if self.manual_board_scroll {
            layout_state.focused_thought = None;
        }
        let editor = self.editor_snapshot();
        let first = compute_layout(
            &layout_state,
            editor.as_ref(),
            area,
            self.first_visible,
            &self.expanded,
        );
        let height = self
            .state
            .focused_thought
            .and_then(|id| first.thought(id))
            .map_or(first.board.height.max(1), |thought| {
                thought.text_area.height.max(1)
            });
        self.prepare_layout(TextViewport::new(first.content_width, height));
        let editor = self.editor_snapshot();
        let mut layout = compute_layout(
            &layout_state,
            editor.as_ref(),
            area,
            first.first_index,
            &self.expanded,
        );
        let palette_items = self
            .palette
            .as_ref()
            .map_or(0, palette::PaletteState::match_count);
        let preferred_rows = if self.help {
            9
        } else if self.palette.is_some() {
            palette_items.max(2)
        } else {
            0
        };
        layout.configure_overlay(palette_items, preferred_rows);
        layout.configure_agent_controls(&self.agent_targets);
        let final_height = self
            .state
            .focused_thought
            .and_then(|id| layout.thought(id))
            .map_or(layout.board.height.max(1), |thought| {
                thought.text_area.height.max(1)
            });
        self.prepare_layout(TextViewport::new(layout.content_width, final_height));
        self.first_visible = layout.first_index;
        self.layout = Some(layout.clone());
        layout
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
        let content = thought.content.clone();
        let restored_cursor = self.state.restored_editor_cursor(thought_id);
        if let Some((current, editor)) = &mut self.editor
            && *current == thought_id
        {
            if editor.snapshot().content != content {
                editor.replace_content(content, restored_cursor.unwrap_or_default());
            }
        } else {
            let mut editor = self.editor_factory.create(&content);
            editor.set_viewport(self.viewport);
            if let Some(cursor) = restored_cursor {
                editor.replace_content(content, cursor);
            } else {
                let _outcome = editor.apply(EditCommand::Move {
                    movement: CursorMovement::DocumentEnd,
                    extend_selection: false,
                });
            }
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

    pub(super) fn request_quit(&mut self) {
        if matches!(
            self.state.durability,
            DurabilityState::Failed { failed, .. }
                if self.recovery_exported_for != Some(failed)
        ) {
            self.status = Some("retry the save or export recovery before quitting".to_owned());
        } else {
            self.quit = true;
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
        let action = Action::EditThought {
            thought_id,
            revision_id: ids.revision_id(),
            before_content: before.content,
            after_content: after.content,
            before_cursor: before.cursor,
            after_cursor: after.cursor,
            at: clock.now(),
        };
        match reduce(&mut self.state, action) {
            Ok(effects) => effects,
            Err(error) => {
                self.status = Some(error.to_string());
                self.reload_editor();
                Vec::new()
            }
        }
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
