//! Responsive board geometry and layout-derived hit targets.

mod chrome;

use std::collections::BTreeSet;

use ratatui_core::layout::Rect;

use crate::{
    application::AppState,
    domain::{Direction, ThoughtId},
    ports::{agent::AgentTarget, editor::EditorSnapshot, text_layout::wrap_rows},
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
    /// Submit to one verified direction, optionally removing after acceptance.
    Submit(Direction, bool),
    /// Board undo action.
    Undo,
    /// Contextual help action.
    Help,
    /// Clean exit action.
    Quit,
    /// Leave the editor or discard an unchanged draft.
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
    /// Integration, mode, and durability context row.
    pub footer_context: Rect,
    /// Contextual labeled actions.
    pub footer_actions: Rect,
    /// Optional dedicated transient-status row.
    pub footer_status: Option<Rect>,
    /// Visible thought allocations.
    pub thoughts: Vec<ThoughtLayout>,
    /// Clickable insertion control when visible.
    pub insert: Option<Rect>,
    /// First visible live thought.
    pub first_index: usize,
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
    /// Resolve one terminal cell through the same rectangles used to render.
    #[must_use]
    pub fn hit_test(&self, column: u16, row: u16) -> Option<HitTarget> {
        if let Some(overlay) = &self.overlay {
            if contains(overlay.close, column, row) {
                return Some(HitTarget::CloseOverlay);
            }
            return overlay.items.iter().enumerate().find_map(|(index, area)| {
                contains(*area, column, row).then_some(HitTarget::PaletteItem(index))
            });
        }
        for thought in &self.thoughts {
            if contains(thought.gutter, column, row) {
                return Some(HitTarget::DragHandle(thought.thought_id));
            }
            if thought
                .overflow
                .is_some_and(|area| contains(area, column, row))
            {
                return Some(HitTarget::Overflow(thought.thought_id));
            }
            if contains(thought.text_area, column, row) {
                return Some(HitTarget::Thought(thought.thought_id));
            }
        }
        if self.insert.is_some_and(|area| contains(area, column, row)) {
            return Some(HitTarget::Insert);
        }
        self.controls
            .iter()
            .find_map(|(target, area)| contains(*area, column, row).then_some(*target))
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
        self.overlay =
            (preferred_rows > 0).then(|| overlay_layout(self.area, item_count, preferred_rows));
    }

    /// Add only currently verified agent controls where footer width permits.
    pub fn configure_agent_controls(&mut self, targets: &[AgentTarget]) {
        let entries = targets
            .iter()
            .flat_map(|target| [(target.direction, false), (target.direction, true)])
            .collect::<Vec<_>>();
        let required = u16::try_from(entries.len())
            .unwrap_or(u16::MAX)
            .saturating_mul(8);
        while controls_end(&self.controls, self.footer_actions.x).saturating_add(required)
            > self.footer_actions.right()
        {
            if self.controls.pop().is_none() {
                return;
            }
        }
        let mut left = controls_end(&self.controls, self.footer_actions.x);
        for (direction, remove) in entries {
            self.controls.push((
                HitTarget::Submit(direction, remove),
                Rect::new(left, self.footer_actions.y, 8, 1),
            ));
            left = left.saturating_add(8);
        }
    }
}

fn controls_end(controls: &[(HitTarget, Rect)], fallback: u16) -> u16 {
    controls.last().map_or(fallback, |(_, area)| area.right())
}

/// Compute responsive geometry from current state and terminal dimensions.
#[must_use]
pub fn compute(
    state: &AppState,
    editor: Option<&EditorSnapshot>,
    area: Rect,
    requested_first: usize,
    expanded: &BTreeSet<ThoughtId>,
    has_status: bool,
) -> LayoutSnapshot {
    let chrome = chrome::compute(area, state.mode, has_status);
    let board = chrome.board;
    let content_width = board.width.saturating_sub(2).max(1);
    let live = state.board.live_thoughts();
    let focus_index = state
        .focused_thought
        .and_then(|id| live.iter().position(|thought| thought.id == id));
    let mut first = requested_first.min(live.len().saturating_sub(1));
    if focus_index.is_some_and(|index| index < first) {
        first = focus_index.unwrap_or(first);
    }
    let mut thoughts = place_thoughts(state, editor, board, content_width, first, expanded);
    if focus_index.is_some_and(|index| !thoughts.iter().any(|layout| layout.index == index)) {
        first = focus_index.unwrap_or(first);
        thoughts = place_thoughts(state, editor, board, content_width, first, expanded);
    }
    let used_bottom = thoughts
        .last()
        .map_or(board.y, |layout| layout.area.bottom());
    let insert = (matches!(state.mode, crate::application::InteractionMode::Board)
        && used_bottom < board.bottom())
    .then(|| Rect::new(board.x, used_bottom, board.width, 1));
    LayoutSnapshot {
        area,
        board,
        header: chrome.header,
        footer: chrome.footer,
        footer_context: chrome.context,
        footer_actions: chrome.actions,
        footer_status: chrome.status,
        thoughts,
        insert,
        first_index: first,
        controls: chrome::controls(
            chrome.actions,
            state.mode,
            matches!(
                state.durability,
                crate::application::DurabilityState::Failed { .. }
            ),
            state.focused_thought.is_some(),
        ),
        content_width,
        overlay: None,
    }
}

fn overlay_layout(area: Rect, item_count: usize, preferred_rows: usize) -> OverlayLayout {
    let width = area.width.clamp(1, 58);
    let requested_height = u16::try_from(preferred_rows.saturating_add(3)).unwrap_or(u16::MAX);
    let height = area.height.clamp(1, requested_height.max(5));
    let modal = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let items = (0..item_count.min(usize::from(height.saturating_sub(3))))
        .map(|index| {
            Rect::new(
                modal.x.saturating_add(1),
                modal
                    .y
                    .saturating_add(2)
                    .saturating_add(u16::try_from(index).unwrap_or(u16::MAX)),
                modal.width.saturating_sub(2),
                1,
            )
        })
        .collect();
    OverlayLayout {
        area: modal,
        items,
        close: Rect::new(modal.right().saturating_sub(3), modal.y, 3, 1),
    }
}

fn place_thoughts(
    state: &AppState,
    editor: Option<&EditorSnapshot>,
    board: Rect,
    content_width: u16,
    first: usize,
    expanded: &BTreeSet<ThoughtId>,
) -> Vec<ThoughtLayout> {
    let mut layouts = Vec::new();
    let mut y = board.y;
    let cap = responsive_cap(board.height);
    let live = state.board.live_thoughts();
    let remaining = live.len().saturating_sub(first);
    let roomy = usize::from(board.height)
        >= remaining.saturating_add(remaining.saturating_sub(1).saturating_mul(3));
    for (index, thought) in live.into_iter().enumerate().skip(first) {
        if y >= board.bottom() {
            break;
        }
        let separation = if roomy { 3 } else { 1 };
        if !layouts.is_empty() && y.saturating_add(separation) >= board.bottom() {
            break;
        }
        let separator_before = separator_before(board, &mut y, !layouts.is_empty(), roomy);
        let editing = editor.is_some_and(|_| state.focused_thought == Some(thought.id));
        let natural = editor
            .filter(|_| state.focused_thought == Some(thought.id))
            .map_or_else(
                || wrapped_rows(&thought.content, content_width),
                |snapshot| snapshot.visual_lines.len().max(1),
            );
        let explicit_cap = if expanded.contains(&thought.id) {
            usize::from(board.height.max(1))
        } else if thought.collapsed {
            2
        } else {
            usize::from(cap)
        };
        let visible_rows = natural.min(explicit_cap.max(1));
        let available = usize::from(board.bottom().saturating_sub(y));
        let height = u16::try_from(visible_rows.min(available).max(1)).unwrap_or(u16::MAX);
        let area = Rect::new(board.x, y, board.width, height);
        let gutter = Rect::new(area.x, area.y, area.width.min(1), area.height);
        let text_area = Rect::new(
            area.x.saturating_add(2).min(area.right()),
            area.y,
            area.width.saturating_sub(2),
            area.height,
        );
        let hidden_rows = if editing {
            0
        } else if natural > usize::from(height) {
            natural.saturating_sub(usize::from(height).saturating_sub(1))
        } else {
            0
        };
        let overflow = (hidden_rows > 0).then(|| {
            Rect::new(
                text_area.x,
                area.bottom().saturating_sub(1),
                text_area.width,
                1,
            )
        });
        layouts.push(ThoughtLayout {
            thought_id: thought.id,
            index,
            separator_before,
            area,
            text_area,
            gutter,
            overflow,
            hidden_rows,
        });
        y = y.saturating_add(height);
    }
    layouts
}

fn separator_before(board: Rect, y: &mut u16, preceded: bool, roomy: bool) -> Option<Rect> {
    preceded.then(|| {
        let rule_y = if roomy { y.saturating_add(1) } else { *y };
        let separator = Rect::new(
            board.x.saturating_add(2).min(board.right()),
            rule_y,
            board.width.saturating_sub(2),
            1,
        );
        *y = y.saturating_add(if roomy { 3 } else { 1 });
        separator
    })
}

fn responsive_cap(board_height: u16) -> u16 {
    board_height.saturating_mul(2).div_ceil(3).max(3)
}

fn wrapped_rows(content: &str, width: u16) -> usize {
    wrap_rows(content, usize::from(width.max(1))).len().max(1)
}

const fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{HitTarget, compute};
    use crate::{
        application::AppState,
        domain::{Session, SessionBoard, SessionId, Timestamp},
    };
    use ratatui_core::layout::Rect;

    fn empty_state() -> AppState {
        let now = Timestamp::from_millis(1);
        let session = Session::new(
            SessionId::from_uuid(uuid::Uuid::now_v7()).expect("UUIDv7 session ID"),
            "/tmp".into(),
            now,
        )
        .expect("session");
        AppState::new(SessionBoard::new(session, Vec::new()).expect("board"))
    }

    #[test]
    fn empty_layout_exposes_shared_footer_and_insert_targets() {
        let layout = compute(
            &empty_state(),
            None,
            Rect::new(0, 0, 20, 5),
            0,
            &BTreeSet::new(),
            false,
        );
        assert_eq!(layout.header, Rect::new(0, 0, 20, 1));
        assert_eq!(layout.hit_test(0, 1), Some(HitTarget::Insert));
        assert!(
            layout
                .controls
                .iter()
                .any(|(target, _)| { matches!(target, HitTarget::Commands | HitTarget::Help) })
        );
    }
}
