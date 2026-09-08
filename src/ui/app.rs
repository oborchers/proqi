//! Terminal-independent board interaction state.

mod admission;
mod agent;
mod agent_delivery;
mod agent_identity;
mod agent_preparation;
mod attachments;
mod boundary_insertion;
mod clipboard;
mod commands;
mod control;
mod duplicate;
mod editing;
mod folds;
pub(in crate::ui) mod global_delivery;
mod help;
pub(in crate::ui) mod highlights;
mod input_dispatch;
mod invocation;
mod palette;
mod palette_handoff;
mod pending_types;
mod pointer;
mod pointer_activation;
mod pointer_editor;
mod presentation;
mod query;
mod recovery;
mod reflow;
mod reorder;
mod screenshot;
mod search;
mod selection;
mod session;
mod state_bridge;
mod transfer;
mod transformations;
mod update;
mod view;
mod view_frame;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use crate::{
    application::{AppState, Effect, InteractionMode},
    domain::{OperationId, OperationSequence, RequestId, SubmissionId, ThoughtId},
    ports::{
        agent::AgentTarget,
        editor::{CursorMovement, EditCommand, Editor, EditorFactory, TextViewport},
        environment::{Clock, IdGenerator},
        invocation::InvocationCompletenessAggregate,
    },
};

use super::{
    HitTarget, LayoutSnapshot, UiSettings,
    input::{PointerButton, PointerInput, PointerKind, RoutedInput as UiInput, UiKey},
    layout::scroll::{BoardViewport, ScrollGeometry},
};

use input_dispatch::ActiveInputOwner as Owner;
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
    Armed(BoundaryInsertion),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BoundaryInsertion {
    BeforeFirst,
    AfterLast,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorOwner {
    Compose,
    Thought(ThoughtId),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ComposePresentation {
    #[default]
    Prompt,
    Editor,
}

/// Mutable UI state around the pure application reducer.
pub struct BoardApp {
    /// Reducer-owned application state rendered by the board.
    pub state: AppState,
    editor: Option<(EditorOwner, Box<dyn Editor>)>,
    editor_factory: Box<dyn EditorFactory>,
    compose_presentation: ComposePresentation,
    pending_edit: Option<editing::PendingEdit>,
    edit_generation: u64,
    edit_owner_generation: u64,
    compose_generation: u64,
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
    frame_presentation: Option<crate::ui::projection::FramePresentation>,
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
    pending_clipboard_reads: BTreeMap<RequestId, pending_types::PendingClipboardRead>,
    pending_recovery_exports: BTreeSet<RequestId>,
    recovery_exported_for: Option<OperationSequence>,
    agent_targets: Vec<AgentTarget>,
    agent_refresh_in_flight: bool,
    global_delivery: Option<global_delivery::GlobalDeliveryState>,
    global_delivery_generation: u64,
    submission_mode: Option<SubmissionMode>,
    deferred_submissions: BTreeMap<SubmissionId, DeferredSubmissionIntent>,
    preflight_submissions: BTreeMap<SubmissionId, DeferredSubmissionIntent>,
    pending_submissions: BTreeMap<SubmissionId, PendingSubmission>,
    pending_transfer_removals: BTreeSet<OperationId>,
    screenshot: screenshot::ScreenshotInbox,
    update_barrier: Option<update::UpdateBarrier>,
    update_restart: Option<crate::domain::StableVersion>,
    update_prompt: Option<update::UpdatePrompt>,
    release_highlights: Option<highlights::ReleaseHighlightsOverlay>,
    installed_highlights: Option<crate::domain::ReleaseHighlightGroup>,
    invocation_cwd: PathBuf,
    invocation_generation: u64,
    invocation_reference_generation: u64,
    invocation_reference_pending: Option<u64>,
    invocation_global: Vec<crate::ports::invocation::InvocationEntry>,
    invocation_project: Vec<crate::ports::invocation::InvocationEntry>,
    invocation_live: Vec<crate::ports::invocation::LiveAgentReference>,
    invocation_completeness: crate::ports::invocation::InvocationCompletenessAggregate,
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
        let insertion_focus = InsertionFocus::Inactive;
        let editor_factory: Box<dyn EditorFactory> = Box::new(editor_factory);
        let editor = if matches!(state.mode, InteractionMode::Compose) {
            Some((EditorOwner::Compose, editor_factory.create("")))
        } else {
            None
        };
        Self {
            state,
            editor,
            editor_factory,
            compose_presentation: ComposePresentation::Prompt,
            pending_edit: None,
            edit_generation: 0,
            edit_owner_generation: 0,
            compose_generation: 0,
            quit: false,
            help: false,
            help_scroll: 0,
            status: None,
            viewport: TextViewport::default(),
            board_viewport: BoardViewport::default(),
            scroll_geometry: None,
            layout: None,
            frame_presentation: None,
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
            pending_clipboard_reads: BTreeMap::new(),
            pending_recovery_exports: BTreeSet::new(),
            recovery_exported_for: None,
            agent_targets: Vec::new(),
            agent_refresh_in_flight: false,
            global_delivery: None,
            global_delivery_generation: 0,
            submission_mode: None,
            deferred_submissions: BTreeMap::new(),
            preflight_submissions: BTreeMap::new(),
            pending_submissions: BTreeMap::new(),
            pending_transfer_removals: BTreeSet::new(),
            screenshot: screenshot::ScreenshotInbox::default(),
            update_barrier: None,
            update_restart: None,
            update_prompt: None,
            release_highlights: None,
            installed_highlights: None,
            invocation_cwd,
            invocation_generation: 0,
            invocation_reference_generation: 0,
            invocation_reference_pending: None,
            invocation_global: Vec::new(),
            invocation_project: Vec::new(),
            invocation_live: Vec::new(),
            invocation_completeness: InvocationCompletenessAggregate::default(),
        }
    }

    /// Apply normalized input and return ordered external effects.
    pub fn handle(
        &mut self,
        input: super::input::UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        self.handle_routed(UiInput::from(input), ids, clock)
    }

    fn handle_routed(
        &mut self,
        input: UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        let (owner, input, preserves_handoff) = match self.prepare_input(input, ids, clock) {
            Ok(prepared) => prepared,
            Err(effects) => return effects,
        };
        if let Some(effects) = self.handle_quit_input(&input, ids, clock) {
            return effects;
        }
        if self.update_barrier.is_some()
            && !matches!(
                input,
                UiInput::Resize { .. } | UiInput::HostFocusGained | UiInput::HostFocusLost
            )
        {
            return Vec::new();
        }
        self.handle_routable_input(owner, input, preserves_handoff, ids, clock)
    }

    fn prepare_input(
        &mut self,
        input: UiInput,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Result<(input_dispatch::ActiveInputOwner, UiInput, bool), Vec<Effect>> {
        if self.screenshot_save_in_flight() && matches!(input, UiInput::KeyStroke(_)) {
            self.note_screenshot_interaction(&input);
            return Err(self.handle_screenshot_commit_barrier(input, ids, clock));
        }
        if self.update_barrier.is_some() && matches!(input, UiInput::KeyStroke(_)) {
            return Err(Vec::new());
        }
        let (contexts, owner) = self.active_input_route();
        let preserves_handoff = match &input {
            UiInput::KeyStroke(stroke) => self
                .settings
                .shortcuts
                .preserves_editor_handoff(&contexts, *stroke),
            _ => false,
        };
        let Some(input) = self.resolve_shortcut_input(&contexts, input) else {
            return Err(Vec::new());
        };
        let input = self.resolve_edit_navigation(input, owner);
        self.reset_pointer_click_for_input(&input);
        self.note_screenshot_interaction(&input);
        self.reset_overlay_activation_for_input(&input, clock.now());
        if matches!(input, UiInput::HostFocusLost) {
            self.collapse_empty_compose();
        }
        if owner.owns_modal_surface() {
            self.pointer_click = None;
        }
        if self.screenshot_save_in_flight() {
            return Err(self.handle_screenshot_commit_barrier(input, ids, clock));
        }
        Ok((owner, input, preserves_handoff))
    }

    fn handle_routable_input(
        &mut self,
        owner: input_dispatch::ActiveInputOwner,
        input: UiInput,
        preserves_handoff: bool,
        ids: &mut impl IdGenerator,
        clock: &impl Clock,
    ) -> Vec<Effect> {
        if input.is_deliberate_interaction() {
            let acknowledges_auto_pause = owner.acknowledges_screenshot_auto_pause();
            self.acknowledge_screenshot_auto_pause_warning(acknowledges_auto_pause);
            self.clear_status_for_interaction(acknowledges_auto_pause);
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
        match owner {
            Owner::Help => self.handle_help_input(&input),
            Owner::Screenshot => self.handle_screenshot_takeover_input(&input, ids, clock),
            Owner::Update => self.handle_update_prompt_input(&input),
            Owner::ReleaseHighlights => self.handle_release_highlights_input(&input),
            Owner::Commands => self.handle_palette_input(&input, ids, clock),
            Owner::GlobalDeliveryQuery | Owner::GlobalDeliveryDisposition => {
                self.handle_global_delivery_input(&input, ids, clock)
            }
            Owner::Invocation | Owner::InvocationQuery => {
                self.handle_invocation_input(&input, ids, clock)
            }
            Owner::Transfer => self.handle_transfer_input(&input, ids, clock),
            Owner::Rename => self.handle_session_rename(&input),
            Owner::Search => self.handle_search_input(&input, ids, clock),
            Owner::Direction => self
                .handle_submission_input(&input, ids, clock)
                .unwrap_or_else(|| self.handle_primary_input(input, preserves_handoff, ids, clock)),
            Owner::Recovery => self
                .handle_failed_recovery_input(&input, ids, clock)
                .unwrap_or_else(|| self.handle_primary_input(input, preserves_handoff, ids, clock)),
            Owner::Board | Owner::Compose | Owner::Edit | Owner::InsertionBoundary => {
                self.handle_primary_input(input, preserves_handoff, ids, clock)
            }
        }
    }

    pub(crate) fn accept_protected_overlay_input(&self, sequence: u64) -> bool {
        if self.screenshot.takeover.is_some() {
            true
        } else if self.update_prompt.is_some() {
            self.accept_update_input(sequence)
        } else {
            self.accept_release_highlights_input(sequence)
        }
    }

    /// Rebuild the editor adapter when reducer state changes externally.
    pub fn sync_editor_from_state(&mut self) {
        let thought_id = match self.state.mode {
            InteractionMode::Board => {
                self.editor = None;
                return;
            }
            InteractionMode::Compose => {
                if !matches!(self.editor, Some((EditorOwner::Compose, _))) {
                    let mut editor = self.editor_factory.create("");
                    editor.set_viewport(self.viewport);
                    self.editor = Some((EditorOwner::Compose, editor));
                    self.compose_presentation = ComposePresentation::Prompt;
                }
                return;
            }
            InteractionMode::Edit { thought_id } => thought_id,
        };
        let Some(thought) = self.state.board.thought(thought_id) else {
            self.editor = None;
            return;
        };
        let content = thought.content.clone();
        let restored_cursor = self.state.restored_editor_cursor(thought_id);
        if let Some((EditorOwner::Thought(current), editor)) = &mut self.editor
            && *current == thought_id
        {
            if self.pending_edit.is_none() && editor.snapshot().content != content {
                let _outcome = editor.replace_content(content, restored_cursor.unwrap_or_default());
            }
        } else {
            self.edit_owner_generation = self.edit_owner_generation.wrapping_add(1);
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
            self.editor = Some((EditorOwner::Thought(thought_id), editor));
        }
    }
}
