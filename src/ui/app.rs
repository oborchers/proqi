//! Terminal-independent board interaction state.

mod admission;
mod agent;
mod agent_delivery;
mod agent_identity;
mod agent_preparation;
mod attachments;
mod clipboard;
mod commands;
mod control;
mod duplicate;
mod editing;
mod folds;
mod help;
mod invocation;
mod palette;
mod palette_handoff;
mod pending_types;
mod pointer;
mod pointer_activation;
mod presentation;
mod query;
mod recovery;
mod reorder;
mod screenshot;
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
    domain::{OperationId, OperationSequence, RequestId, SubmissionId, ThoughtId},
    ports::{
        agent::AgentTarget,
        editor::{CursorMovement, EditCommand, Editor, EditorFactory, TextViewport},
        environment::{Clock, IdGenerator},
    },
};

use super::{
    HitTarget, LayoutSnapshot, PastePayload, UiSettings,
    input::{PointerButton, PointerInput, PointerKind, UiInput, UiKey},
    layout::scroll::{BoardViewport, ScrollGeometry},
};

pub(in crate::ui) use invocation::InvocationChoiceView;
use pending_types::{
    DeferredSubmissionIntent, PendingEditorClipboard, PendingSubmission, SubmissionMode,
};

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
    board_viewport: BoardViewport,
    scroll_geometry: Option<ScrollGeometry>,
    layout: Option<LayoutSnapshot>,
    dragged_thought: Option<ThoughtId>,
    drag_target: Option<usize>,
    pointer_click: Option<pointer::PointerClick>,
    overlay_activation: Option<pointer_activation::OverlayActivation>,
    hovered: Option<HitTarget>,
    insertion_focus: InsertionFocus,
    insertion_confirmation: InsertionConfirmation,
    edit_boundary: Option<CursorMovement>,
    palette_selection_handoff: Option<palette_handoff::EditorSelectionHandoff>,
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
    deferred_submissions: BTreeMap<SubmissionId, DeferredSubmissionIntent>,
    preflight_submissions: BTreeMap<SubmissionId, DeferredSubmissionIntent>,
    pending_submissions: BTreeMap<SubmissionId, PendingSubmission>,
    pending_transfer_removals: BTreeSet<OperationId>,
    screenshot: screenshot::ScreenshotInbox,
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
            board_viewport: BoardViewport::default(),
            scroll_geometry: None,
            layout: None,
            dragged_thought: None,
            drag_target: None,
            pointer_click: None,
            overlay_activation: None,
            hovered: None,
            insertion_focus,
            insertion_confirmation: InsertionConfirmation::Idle,
            edit_boundary: None,
            palette_selection_handoff: None,
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
            deferred_submissions: BTreeMap::new(),
            preflight_submissions: BTreeMap::new(),
            pending_submissions: BTreeMap::new(),
            pending_transfer_removals: BTreeSet::new(),
            screenshot: screenshot::ScreenshotInbox::default(),
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
        let input = self.resolve_edit_navigation(input);
        self.reset_pointer_click_for_input(&input);
        self.note_screenshot_interaction(&input);
        self.reset_overlay_activation_for_input(&input, clock.now());
        if self.help
            || self.screenshot.takeover.is_some()
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
        if self.screenshot.takeover.is_some() {
            return self.handle_screenshot_takeover_input(&input, ids, clock);
        }
        if self.screenshot_save_in_flight() {
            return self.handle_screenshot_commit_barrier(input, ids, clock);
        }
        if let Some(effects) = self.handle_quit_input(&input, ids, clock) {
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
            self.screenshot.notice_count = 0;
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
        self.invalidate_palette_selection_handoff(&input);
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
    pub fn acknowledge_persistence(
        &mut self,
        sequence: OperationSequence,
        succeeded: bool,
    ) -> Vec<Effect> {
        self.acknowledge_persistence_result(
            sequence,
            succeeded.then_some(()).ok_or(FailureCode::StorageFailed),
        )
    }

    /// Apply a typed ordered persistence result and release durability-gated follow-up work.
    pub fn acknowledge_persistence_result(
        &mut self,
        sequence: OperationSequence,
        result: Result<(), FailureCode>,
    ) -> Vec<Effect> {
        let succeeded = result.is_ok();
        let failure = result.as_ref().err().copied();
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
        self.complete_deferred_submission_durability(failure)
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
        self.board_viewport = self.board_viewport.follow_focus();
        self.scroll_geometry = None;
        self.layout = None;
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
