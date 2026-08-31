//! Responsive board geometry and layout-derived hit targets.

mod chrome;
mod content;
mod controls;
pub(super) mod scroll;

use ratatui_core::layout::Rect;

use crate::{
    application::AppState,
    domain::{Direction, ThoughtId},
    ports::{
        agent::{AgentTarget, SubmissionDisposition},
        editor::EditorSnapshot,
    },
};

/// Semantic target resolved from the latest rendered geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HitTarget {
    /// Text content of one thought.
    Thought(ThoughtId),
    /// Reorder handle for one thought.
    DragHandle(ThoughtId),
    /// Overflow indicator for one capped thought.
    Overflow(ThoughtId),
    /// Active insertion area.
    Insert,
    /// Search current thought content.
    Search,
    /// Searchable command discovery.
    Commands,
    /// Copy the focused thought.
    Copy,
    /// Cut the focused thought after clipboard success.
    Cut,
    /// Delete the focused thought without changing the clipboard.
    Delete,
    /// Toggle the focused thought in the board selection.
    Select,
    /// Display one independently verified adjacent target.
    Agent(Direction),
    /// Rename the current session.
    RenameSession,
    /// Copy the complete canonical current-session identifier.
    CopySessionId,
    /// Submit to one verified direction with explicit post-acceptance behavior.
    Deliver(Direction, SubmissionDisposition),
    /// Choose a verified direction for one submission intention.
    BeginDelivery(SubmissionDisposition),
    /// Board undo action.
    Undo,
    /// Contextual help action.
    Help,
    /// Clean exit action.
    Quit,
    /// Leave the editor.
    ExitEdit,
    /// Retry the failed durable operation.
    Retry,
    /// Export the exact unsaved recovery buffer.
    ExportRecovery,
    /// Search result within the active modal picker.
    PaletteItem(usize),
    /// Close the active help or command overlay.
    CloseOverlay,
}

/// Geometry for one visible thought.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThoughtLayout {
    /// Durable thought identity.
    pub thought_id: ThoughtId,
    /// Live board position.
    pub index: usize,
    /// Quiet non-interactive rule before this thought, when another thought precedes it.
    pub separator_before: Option<Rect>,
    /// Complete visible allocation.
    pub area: Rect,
    /// Text cells excluding the focus or drag gutter.
    pub text_area: Rect,
    /// Stable one-cell drag and focus gutter.
    pub gutter: Rect,
    /// Clickable overflow row when content is capped.
    pub overflow: Option<Rect>,
    /// Number of wrapped rows hidden by the cap.
    pub hidden_rows: usize,
    /// Whether the viewport, rather than the presentation cap, clipped this allocation.
    pub viewport_clipped: bool,
    /// Whether line scrolling may reveal rows hidden from this presentation.
    pub scrollable_hidden: bool,
    /// Wrapped rows clipped above the viewport for line-by-line board scrolling.
    pub content_row_offset: usize,
}

/// Geometry for the transient insertion editor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposeLayout {
    /// Complete visible allocation.
    pub area: Rect,
    /// Editor cells excluding the focus gutter.
    pub text_area: Rect,
    /// Stable one-cell focus gutter.
    pub gutter: Rect,
}

/// Complete geometry used by both rendering and mouse resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutSnapshot {
    /// Complete terminal area.
    pub area: Rect,
    /// Scrollable board area above the footer.
    pub board: Rect,
    /// Quiet product and session identity row.
    pub header: Rect,
    /// Complete footer allocation.
    pub footer: Rect,
    /// Transient status row.
    pub footer_status: Rect,
    /// Durable, always-addressable session name row.
    pub footer_name: Rect,
    /// Mode, thought count, and durability row.
    pub footer_context: Rect,
    /// Contextual labeled actions.
    pub footer_actions: Rect,
    /// Verified adjacent-agent targets.
    pub footer_agents: Rect,
    /// Visible thought allocations.
    pub thoughts: Vec<ThoughtLayout>,
    /// Transient insertion editor geometry when Compose is active.
    pub compose: Option<ComposeLayout>,
    /// Clickable insertion control when visible.
    pub insert: Option<Rect>,
    /// First visible live thought.
    pub first_index: usize,
    /// Wrapped row offset within the first visible thought.
    pub first_row_offset: usize,
    /// Greatest useful first-visible thought for the current content and viewport.
    pub max_first_index: usize,
    /// Responsive right-aligned session and board summary.
    pub footer_summary: String,
    /// Responsive session identity.
    pub footer_session_name: String,
    /// Complete session identifier when it fits beside the untruncated name.
    pub footer_session_id: Option<String>,
    /// Footer command targets.
    pub controls: Vec<(HitTarget, Rect)>,
    /// Content width supplied to the editor.
    pub content_width: u16,
    /// Modal help or command geometry, when visible.
    pub overlay: Option<OverlayLayout>,
}

/// Geometry for a centered modal overlay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverlayLayout {
    /// Complete bordered overlay.
    pub area: Rect,
    /// Visible command rows.
    pub items: Vec<Rect>,
    /// Stable close target in the upper-right corner.
    pub close: Rect,
}

impl LayoutSnapshot {
    /// Store the rendered footer summary and register its visible session-name target.
    pub fn configure_footer_summary(
        &mut self,
        summary: String,
        session_name: String,
        session_id: Option<String>,
    ) {
        controls::configure_footer_summary(self, summary, session_name, session_id);
    }

    /// Resolve one terminal cell through the same rectangles used to render.
    #[must_use]
    pub fn hit_test(&self, column: u16, row: u16) -> Option<HitTarget> {
        if let Some(overlay) = &self.overlay {
            if crate::ui::geometry::contains(overlay.close, column, row) {
                return Some(HitTarget::CloseOverlay);
            }
            return overlay.items.iter().enumerate().find_map(|(index, area)| {
                crate::ui::geometry::contains(*area, column, row)
                    .then_some(HitTarget::PaletteItem(index))
            });
        }
        for thought in &self.thoughts {
            if crate::ui::geometry::contains(thought.gutter, column, row) {
                return Some(HitTarget::DragHandle(thought.thought_id));
            }
            if thought
                .overflow
                .is_some_and(|area| crate::ui::geometry::contains(area, column, row))
            {
                return Some(HitTarget::Overflow(thought.thought_id));
            }
            if crate::ui::geometry::contains(thought.text_area, column, row) {
                return Some(HitTarget::Thought(thought.thought_id));
            }
        }
        if self
            .compose
            .as_ref()
            .is_some_and(|compose| crate::ui::geometry::contains(compose.area, column, row))
        {
            return Some(HitTarget::Insert);
        }
        if self
            .insert
            .is_some_and(|area| crate::ui::geometry::contains(area, column, row))
        {
            return Some(HitTarget::Insert);
        }
        self.controls.iter().find_map(|(target, area)| {
            crate::ui::geometry::contains(*area, column, row).then_some(*target)
        })
    }

    /// Find current visible geometry for a thought.
    #[must_use]
    pub fn thought(&self, thought_id: ThoughtId) -> Option<&ThoughtLayout> {
        self.thoughts
            .iter()
            .find(|layout| layout.thought_id == thought_id)
    }

    /// Map a board row to the nearest visible thought position.
    #[must_use]
    pub fn insertion_index_at(&self, row: u16) -> Option<usize> {
        self.thoughts
            .iter()
            .find(|layout| row < layout.area.bottom())
            .map(|layout| layout.index)
            .or_else(|| self.thoughts.last().map(|layout| layout.index))
    }

    /// Attach modal geometry after application overlays are known.
    pub fn configure_overlay(&mut self, item_count: usize, preferred_rows: usize) {
        self.overlay = (preferred_rows > 0).then(|| {
            let required = controls::overlay_height(preferred_rows);
            let covers_chrome = self.board.height < required;
            let bounds = if covers_chrome { self.area } else { self.board };
            controls::overlay_layout(bounds, item_count, preferred_rows, covers_chrome)
        });
    }

    /// Add only currently verified agent controls where footer width permits.
    pub fn configure_agent_controls(
        &mut self,
        targets: &[AgentTarget],
        selection: Option<SubmissionDisposition>,
    ) {
        controls::configure_agent_controls(
            self,
            targets,
            selection,
            crate::application::InteractionMode::Board,
            &crate::ui::KeyBindings::default(),
        );
    }

    pub(crate) fn configure_agent_controls_with_keys(
        &mut self,
        targets: &[AgentTarget],
        selection: Option<SubmissionDisposition>,
        mode: crate::application::InteractionMode,
        keybindings: &crate::ui::KeyBindings,
    ) {
        controls::configure_agent_controls(self, targets, selection, mode, keybindings);
    }
}

/// Compute responsive geometry from current state and terminal dimensions.
#[must_use]
pub fn compute(
    state: &AppState,
    editor: Option<&EditorSnapshot>,
    area: Rect,
    requested_first: usize,
    insertion_focused: bool,
    has_agents: bool,
) -> LayoutSnapshot {
    compute_with_density(
        state,
        editor,
        area,
        requested_first,
        insertion_focused,
        has_agents,
        false,
        crate::ui::settings::BoardDensity::Comfortable,
        0,
        &crate::ui::KeyBindings::default(),
    )
}

/// Compute board geometry with an explicit spacing preference.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "layout inputs are independent viewport contracts"
)]
pub fn compute_with_density(
    state: &AppState,
    editor: Option<&EditorSnapshot>,
    area: Rect,
    requested_first: usize,
    insertion_focused: bool,
    has_agents: bool,
    has_status: bool,
    density: crate::ui::settings::BoardDensity,
    requested_row_offset: usize,
    keybindings: &crate::ui::KeyBindings,
) -> LayoutSnapshot {
    compute_frame(
        state,
        editor,
        area,
        requested_first,
        insertion_focused,
        has_agents,
        has_status,
        density,
        requested_row_offset,
        keybindings,
        None,
    )
    .0
}

#[expect(
    clippy::too_many_arguments,
    reason = "layout inputs are independent viewport contracts"
)]
pub(super) fn compute_for_app(
    state: &AppState,
    editor: Option<&EditorSnapshot>,
    area: Rect,
    insertion_focused: bool,
    has_agents: bool,
    has_status: bool,
    density: crate::ui::settings::BoardDensity,
    keybindings: &crate::ui::KeyBindings,
    viewport: scroll::BoardViewport,
) -> (LayoutSnapshot, scroll::ScrollGeometry) {
    compute_frame(
        state,
        editor,
        area,
        0,
        insertion_focused,
        has_agents,
        has_status,
        density,
        0,
        keybindings,
        Some(viewport),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "layout inputs are independent viewport contracts"
)]
fn compute_frame(
    state: &AppState,
    editor: Option<&EditorSnapshot>,
    area: Rect,
    requested_first: usize,
    insertion_focused: bool,
    has_agents: bool,
    has_status: bool,
    density: crate::ui::settings::BoardDensity,
    requested_row_offset: usize,
    keybindings: &crate::ui::KeyBindings,
    viewport: Option<scroll::BoardViewport>,
) -> (LayoutSnapshot, scroll::ScrollGeometry) {
    let chrome = chrome::compute(area, has_agents, has_status);
    let board = chrome.board;
    let content_width = board.width.saturating_sub(2).max(1);
    let content = content::visible_content(&content::ContentRequest {
        state,
        editor,
        board,
        content_width,
        insertion_focused,
        density,
        viewport,
        requested_first,
        requested_row_offset,
    });
    let scroll = content.scroll;
    let layout = LayoutSnapshot {
        area,
        board,
        header: chrome.header,
        footer: chrome.footer,
        footer_status: chrome.status,
        footer_name: chrome.name,
        footer_context: chrome.state,
        footer_actions: chrome.actions,
        footer_agents: chrome.agents,
        thoughts: content.thoughts,
        compose: content.compose,
        insert: content.insert,
        first_index: content.first,
        first_row_offset: content.first_row_offset,
        max_first_index: content.max_first,
        footer_summary: String::new(),
        footer_session_name: String::new(),
        footer_session_id: None,
        controls: chrome::controls(
            chrome.actions,
            state.mode,
            matches!(
                state.durability,
                crate::application::DurabilityState::Failed { .. }
            ),
            !matches!(
                state.durability,
                crate::application::DurabilityState::Failed {
                    code: crate::application::FailureCode::RecoveryCapacity,
                    ..
                }
            ),
            state.focused_thought.is_some(),
            keybindings,
        ),
        content_width,
        overlay: None,
    };
    (layout, scroll)
}

#[cfg(test)]
mod tests;
