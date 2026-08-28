//! Terminal-independent board interaction state.

mod agent;
mod agent_identity;
mod clipboard;
mod commands;
mod control;
mod duplicate;
mod editing;
mod folds;
mod help;
mod invocation;
mod palette;
mod pending_types;
mod pointer;
mod presentation;
mod query;
mod recovery;
mod reorder;
mod search;
mod selection;
mod session;
mod transfer;
mod update;
mod view;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use crate::{
    application::{
        Action, AppState, DurabilityState, Effect, FailureCode, InteractionMode, reduce,
    },
    domain::{OperationSequence, RequestId, SubmissionId, ThoughtId},
    ports::{
        agent::AgentTarget,
        editor::{CursorMovement, EditCommand, Editor, EditorFactory, TextViewport},
        environment::{Clock, IdGenerator},
    },
};

use super::{
    HitTarget, LayoutSnapshot, PastePayload, UiSettings,
    input::{PointerButton, PointerInput, PointerKind, UiInput, UiKey},
};

pub(in crate::ui) use invocation::InvocationChoiceView;
use pending_types::{PendingEditorClipboard, PendingSubmission, SubmissionMode};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum InsertionFocus {
    #[default]
    Inactive,
    Active,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum InsertionConfirmation {
    #[default]
    Idle,
    Armed,
}

/// Mutable UI state around the pure application reducer.
pub struct BoardApp {
    /// Reducer-owned application state rendered by the board.
    pub state: AppState,
    editor: Option<(ThoughtId, Box<dyn Editor>)>,
    editor_factory: Box<dyn EditorFactory>,
    pending_edit: Option<editing::PendingEdit>,
    edit_generation: u64,
    /// Whether the user requested a clean exit.
    pub quit: bool,
    /// Whether contextual help is visible.
    pub help: bool,
    help_scroll: usize,
    /// Transient human-readable status.
    pub(in crate::ui) status: Option<crate::ui::status::UiStatus>,
    viewport: TextViewport,
    first_visible: usize,
    first_visible_row: usize,
    manual_board_scroll: bool,
    layout: Option<LayoutSnapshot>,
    dragged_thought: Option<ThoughtId>,
    drag_target: Option<usize>,
    pointer_click: Option<pointer::PointerClick>,
    hovered: Option<HitTarget>,
    insertion_focus: InsertionFocus,
    insertion_confirmation: InsertionConfirmation,
    edit_boundary: Option<CursorMovement>,
    palette: Option<palette::PaletteState>,
    invocation_popup: Option<invocation::InvocationPopup>,
    search: Option<search::SearchState>,
    rename: Option<String>,
    transfer: Option<transfer::TransferState>,
    settings: UiSettings,
    selection: selection::BoardSelection,
    expanded_folds: BTreeSet<(ThoughtId, usize)>,
    pending_editor_clipboard: BTreeMap<RequestId, PendingEditorClipboard>,
    pending_session_clipboard: BTreeMap<RequestId, crate::application::ClipboardIntent>,
    pending_clipboard_reads: BTreeSet<RequestId>,
    pending_recovery_exports: BTreeSet<RequestId>,
    recovery_exported_for: Option<OperationSequence>,
    agent_targets: Vec<AgentTarget>,
    submission_mode: Option<SubmissionMode>,
    pending_submissions: BTreeMap<SubmissionId, PendingSubmission>,
    update_barrier: Option<update::UpdateBarrier>,
    update_restart: Option<crate::domain::StableVersion>,
    update_prompt: Option<update::UpdatePrompt>,
    invocation_cwd: PathBuf,
    invocation_generation: u64,
    invocation_global: Vec<crate::ports::invocation::InvocationEntry>,
    invocation_project: Vec<crate::ports::invocation::InvocationEntry>,
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
        Self::with_settings_and_cwd(state, settings, PathBuf::new(), editor_factory)
    }

    /// Construct a board with validated settings and an explicit discovery cwd.
    #[must_use]
    pub fn with_settings_and_cwd(
        state: AppState,
        settings: UiSettings,
        invocation_cwd: PathBuf,
        editor_factory: impl EditorFactory + 'static,
    ) -> Self {
        let insertion_focus = if state.board.live_thoughts().is_empty() {
            InsertionFocus::Active
        } else {
            InsertionFocus::Inactive
        };
        Self {
            state,
            editor: None,
            editor_factory: Box::new(editor_factory),
            pending_edit: None,
            edit_generation: 0,
            quit: false,
            help: false,
            help_scroll: 0,
            status: None,
            viewport: TextViewport::default(),
            first_visible: 0,
            first_visible_row: 0,
            manual_board_scroll: false,
            layout: None,
            dragged_thought: None,
            drag_target: None,
            pointer_click: None,
            hovered: None,
            insertion_focus,
            insertion_confirmation: InsertionConfirmation::Idle,
            edit_boundary: None,
            palette: None,
            invocation_popup: None,
            search: None,
            rename: None,
            transfer: None,
            settings,
            selection: selection::BoardSelection::default(),
            expanded_folds: BTreeSet::new(),
            pending_editor_clipboard: BTreeMap::new(),
            pending_session_clipboard: BTreeMap::new(),
            pending_clipboard_reads: BTreeSet::new(),
            pending_recovery_exports: BTreeSet::new(),
            recovery_exported_for: None,
            agent_targets: Vec::new(),
            submission_mode: None,
            pending_submissions: BTreeMap::new(),
            update_barrier: None,
            update_restart: None,
            update_prompt: None,
            invocation_cwd,
            invocation_generation: 0,
            invocation_global: Vec::new(),
            invocation_project: Vec::new(),
        }
    }

    /// Apply normalized input and return ordered external effects.
    pub fn handle(
        &mut self,
        input: UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.reset_pointer_click_for_input(&input);
        if self.help
            || self.update_prompt.is_some()
            || self.palette.is_some()
            || self.invocation_popup.is_some()
            || self.transfer.is_some()
            || self.rename.is_some()
            || self.search.is_some()
            || self.submission_mode.is_some()
        {
            self.pointer_click = None;
        }
        if self.help {
            return self.handle_help_input(&input);
        }
        if input == UiInput::Key(UiKey::Quit) || self.is_failed_recovery_quit(&input) {
            let effects = if matches!(self.state.durability, DurabilityState::Failed { .. }) {
                Vec::new()
            } else {
                self.flush_pending_edit(ids, clock)
            };
            self.request_quit();
            return effects;
        }
        if self.update_prompt.is_some() {
            return self.handle_update_prompt_input(&input);
        }
        if self.update_barrier.is_some()
            && !matches!(input, UiInput::Resize { .. } | UiInput::HostFocusGained)
        {
            return Vec::new();
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
        self.reset_insertion_confirmation(&input);
        if self.palette.is_some() {
            return self.handle_palette_input(&input, ids, clock);
        }
        if self.invocation_popup.is_some() {
            return self.handle_invocation_input(&input, ids, clock);
        }
        if self.transfer.is_some() {
            return self.handle_transfer_input(&input, ids, clock);
        }
        if self.rename.is_some() {
            return self.handle_session_rename(&input);
        }
        if self.search.is_some() {
            return self.handle_search_input(&input, ids, clock);
        }
        if self.submission_mode.is_some()
            && let Some(effects) = self.handle_submission_input(&input, ids, clock)
        {
            return effects;
        }
        if let Some(effects) = self.handle_failed_recovery_input(&input, ids, clock) {
            return effects;
        }
        self.handle_primary_input(input, ids, clock)
    }

    fn handle_primary_input(
        &mut self,
        input: UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        match input {
            UiInput::HostFocusGained => Self::discover_agents(),
            UiInput::Resize { .. } => {
                self.layout = None;
                self.hovered = None;
                self.edit_boundary = None;
                Vec::new()
            }
            UiInput::Pointer(pointer) => self.handle_pointer(pointer, ids, clock),
            UiInput::Paste(content) => {
                let effects = self.paste_payload(PastePayload::text(content), ids, clock);
                self.refresh_invocation_popup();
                effects
            }
            UiInput::PasteAnnotated(payload) => {
                let effects = self.paste_payload(payload, ids, clock);
                self.refresh_invocation_popup();
                effects
            }
            UiInput::Key(key) => match self.interaction_mode() {
                InteractionMode::Board => self.handle_board_key(key, ids, clock),
                InteractionMode::Edit { .. } => {
                    let effects = self.handle_edit_key(key, ids, clock);
                    self.refresh_invocation_popup();
                    effects
                }
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
                let _outcome = editor.replace_content(content, restored_cursor.unwrap_or_default());
            }
        } else {
            let mut editor = self.editor_factory.create(&content);
            editor.set_viewport(self.viewport);
            if let Some(cursor) = restored_cursor {
                let _outcome = editor.replace_content(content, cursor);
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
        self.acknowledge_persistence_result(
            sequence,
            succeeded.then_some(()).ok_or(FailureCode::StorageFailed),
        );
    }

    /// Apply a typed ordered persistence result to the reducer state.
    pub fn acknowledge_persistence_result(
        &mut self,
        sequence: OperationSequence,
        result: Result<(), FailureCode>,
    ) {
        let succeeded = result.is_ok();
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
                code: result.err().unwrap_or(FailureCode::StorageFailed),
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
            self.set_error("retry the save or export recovery before quitting");
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
        self.clear_board_selection();
        self.insertion_focus = InsertionFocus::Inactive;
        self.insertion_confirmation = InsertionConfirmation::Idle;
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

    fn expand_and_enter_edit(
        &mut self,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let Some(thought_id) = self.state.focused_thought else {
            return Vec::new();
        };
        if self.submission_locked(thought_id) {
            self.set_warning("thought has a submission in progress");
            return Vec::new();
        }
        let effects = self.expand_thought(thought_id, ids, clock);
        self.enter_edit();
        effects
    }

    fn enter_edit(&mut self) {
        self.insertion_focus = InsertionFocus::Inactive;
        self.edit_boundary = None;
        if let Some(thought_id) = self.state.focused_thought {
            if self.submission_locked(thought_id) {
                self.set_warning("thought has a submission in progress");
                return;
            }
            self.clear_board_selection();
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
            Ok(effects) => {
                let order = self
                    .state
                    .board
                    .live_thoughts()
                    .into_iter()
                    .map(|thought| thought.id)
                    .collect::<Vec<_>>();
                self.selection.reconcile(&order);
                effects
            }
            Err(error) => {
                self.set_error(error.to_string());
                Vec::new()
            }
        }
    }
}
