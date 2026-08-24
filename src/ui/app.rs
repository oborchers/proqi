//! Terminal-independent board interaction state.

mod agent;
mod clipboard;
mod commands;
mod control;
mod draft;
mod editing;
mod folds;
mod palette;
mod pointer;
mod recovery;
mod search;
mod view;

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    application::{
        Action, AppState, ClipboardIntent, DurabilityState, Effect, FailureCode, InteractionMode,
        reduce,
    },
    domain::{OperationSequence, RequestId, SubmissionId, ThoughtId},
    ports::{
        agent::{AgentDeliveryMode, AgentTarget, SubmissionRequest},
        editor::{
            CursorMovement, EditCommand, Editor, EditorFactory, EditorSnapshot, TextViewport,
        },
        environment::{Clock, IdGenerator},
    },
};

use super::{HitTarget, LayoutSnapshot, PastePayload, UiSettings};

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
    delivery: AgentDeliveryMode,
    remove: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum InsertionFocus {
    #[default]
    Inactive,
    Active,
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
    /// One complete paste with adapter-derived presentation provenance.
    PasteAnnotated(PastePayload),
    /// Latest terminal cell dimensions.
    Resize {
        /// Latest reported terminal width.
        width: u16,
        /// Latest reported terminal height.
        height: u16,
    },
    /// Terminal host focus returned to this pane.
    HostFocusGained,
    /// One normalized mouse or trackpad event.
    Pointer(PointerInput),
}

/// Mutable UI state around the pure application reducer.
pub struct BoardApp {
    /// Reducer-owned application state rendered by the board.
    pub state: AppState,
    editor: Option<(ThoughtId, Box<dyn Editor>)>,
    draft: Option<draft::DraftState>,
    editor_factory: Box<dyn EditorFactory>,
    pending_edit: Option<editing::PendingEdit>,
    edit_generation: u64,
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
    insertion_focus: InsertionFocus,
    edit_boundary: Option<CursorMovement>,
    palette: Option<palette::PaletteState>,
    search: Option<search::SearchState>,
    settings: UiSettings,
    expanded: BTreeSet<ThoughtId>,
    expanded_folds: BTreeSet<(ThoughtId, usize)>,
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
            draft: None,
            editor_factory: Box::new(editor_factory),
            pending_edit: None,
            edit_generation: 0,
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
            insertion_focus: InsertionFocus::Inactive,
            edit_boundary: None,
            palette: None,
            search: None,
            settings,
            expanded: BTreeSet::new(),
            expanded_folds: BTreeSet::new(),
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
            let effects = if matches!(self.state.durability, DurabilityState::Failed { .. }) {
                Vec::new()
            } else {
                self.flush_pending_edit(ids, clock)
            };
            self.discard_draft();
            self.request_quit();
            return effects;
        }
        if !matches!(input, UiInput::Resize { .. } | UiInput::HostFocusGained) {
            self.status = None;
        }
        if !matches!(
            input,
            UiInput::Key(UiKey::Move {
                movement: CursorMovement::VisualUp | CursorMovement::VisualDown,
                extend_selection: false,
            })
        ) {
            self.edit_boundary = None;
        }
        if self.palette.is_some() {
            return self.handle_palette_input(&input, ids, clock);
        }
        if self.search.is_some() {
            return self.handle_search_input(&input, ids, clock);
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
                UiInput::Pointer(pointer) => {
                    return self.handle_recovery_pointer(pointer, ids, clock);
                }
                UiInput::Resize { .. } | UiInput::HostFocusGained => {}
                UiInput::Key(_) | UiInput::Paste(_) | UiInput::PasteAnnotated(_) => {
                    return Vec::new();
                }
            }
        }
        match input {
            UiInput::HostFocusGained => Self::discover_agents(),
            UiInput::Resize { .. } => {
                self.layout = None;
                self.hovered = None;
                self.edit_boundary = None;
                Vec::new()
            }
            UiInput::Pointer(pointer) => self.handle_pointer(pointer, ids, clock),
            UiInput::Paste(content) => self.paste_payload(PastePayload::text(content), ids, clock),
            UiInput::PasteAnnotated(payload) => self.paste_payload(payload, ids, clock),
            UiInput::Key(key) => match self.interaction_mode() {
                InteractionMode::Board => self.handle_board_key(key, ids, clock),
                InteractionMode::Edit { .. } => self.handle_edit_key(key, ids, clock),
            },
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
        let content = thought.content.clone();
        let restored_cursor = self.state.restored_editor_cursor(thought_id);
        if let Some((current, editor)) = &mut self.editor
            && *current == thought_id
        {
            if self.pending_edit.is_none() && editor.snapshot().content != content {
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
        if !succeeded {
            self.quit = false;
        } else if self.pending_edit.is_some() {
            self.edit_generation = self.edit_generation.wrapping_add(1);
        }
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
        self.discard_draft();
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
        payload: PastePayload,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.insertion_focus = InsertionFocus::Inactive;
        if payload.content.is_empty() {
            return self.start_draft(ids, clock);
        }
        self.discard_draft();
        let effects = self.reduce(Action::CreateThought {
            thought_id: ids.thought_id(),
            operation_id: ids.operation_id(),
            content: payload.content,
            annotations: payload.annotations,
            insertion_index: None,
            at: clock.now(),
        });
        self.sync_editor_from_state();
        effects
    }

    fn enter_edit(&mut self) {
        self.insertion_focus = InsertionFocus::Inactive;
        self.edit_boundary = None;
        if self.draft.is_some() {
            return;
        }
        if let Some(thought_id) = self.state.focused_thought {
            let _effects = self.reduce(Action::EnterEdit(thought_id));
            self.sync_editor_from_state();
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
