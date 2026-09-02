//! Responsive thought placement over the canonical visual-row flow.

use ratatui_core::layout::Rect;

use crate::{
    application::AppState, domain::ThoughtPresentation, ui::projection::FramePresentation,
};

use super::{ComposeLayout, ThoughtLayout, scroll};

pub(super) struct ContentRequest<'a> {
    pub(super) state: &'a AppState,
    pub(super) presentation: &'a FramePresentation,
    pub(super) board: Rect,
    pub(super) content_width: u16,
    pub(super) insertion_focused: bool,
    pub(super) density: crate::ui::settings::BoardDensity,
    pub(super) viewport: Option<scroll::BoardViewport>,
    pub(super) requested_first: usize,
    pub(super) requested_row_offset: usize,
}

pub(super) struct VisibleContent {
    pub(super) thoughts: Vec<ThoughtLayout>,
    pub(super) compose: Option<ComposeLayout>,
    pub(super) insert: Option<Rect>,
    pub(super) first: usize,
    pub(super) first_row_offset: usize,
    pub(super) max_first: usize,
    pub(super) scroll: scroll::ScrollGeometry,
}

pub(super) fn visible_content(request: &ContentRequest<'_>) -> VisibleContent {
    let flow = scroll::BoardFlow::measure(
        request.state,
        request.presentation,
        request.content_width,
        request.board.height,
        request.density,
    );
    let viewport = request.viewport.unwrap_or_else(|| {
        scroll::BoardViewport::FollowFocus(
            flow.legacy_anchor(request.requested_first, request.requested_row_offset),
        )
    });
    let resolved = flow.resolve(
        viewport,
        request.state.focused_thought,
        request.insertion_focused,
        request.board.height,
    );
    let board = Rect::new(
        request.board.x,
        request.board.y.saturating_add(flow.top_padding),
        request.board.width,
        request.board.height.saturating_sub(flow.top_padding),
    );
    let thoughts = visible_thoughts(&flow, resolved.offset, board);
    let compose = visible_compose(&flow, resolved.offset, board);
    let insert = flow
        .insert_row
        .filter(|row| visible_row(*row, resolved.offset, board.height))
        .map(|row| {
            Rect::new(
                board.x,
                viewport_y(board, row, resolved.offset),
                board.width,
                1,
            )
        });
    VisibleContent {
        thoughts,
        compose,
        insert,
        first: resolved.first_index,
        first_row_offset: resolved.first_row_offset,
        max_first: resolved.max_first_index,
        scroll: resolved.geometry,
    }
}

fn visible_compose(flow: &scroll::BoardFlow, offset: usize, board: Rect) -> Option<ComposeLayout> {
    let compose = flow.compose.as_ref()?;
    let viewport_end = offset.saturating_add(usize::from(board.height));
    let first = compose.content_start.max(offset);
    let last = compose.end.min(viewport_end);
    if first >= last {
        return None;
    }
    let area = Rect::new(
        board.x,
        viewport_y(board, first, offset),
        board.width,
        u16::try_from(last.saturating_sub(first)).unwrap_or(u16::MAX),
    );
    Some(ComposeLayout {
        area,
        text_area: Rect::new(
            area.x.saturating_add(2).min(area.right()),
            area.y,
            area.width.saturating_sub(2),
            area.height,
        ),
        gutter: Rect::new(area.x, area.y, area.width.min(1), area.height),
    })
}

fn visible_thoughts(flow: &scroll::BoardFlow, offset: usize, board: Rect) -> Vec<ThoughtLayout> {
    let viewport_end = offset.saturating_add(usize::from(board.height));
    flow.thoughts
        .iter()
        .filter_map(|thought| visible_thought(thought, offset, viewport_end, board))
        .collect()
}

#[derive(Clone, Copy)]
struct VisibleRows {
    separator: bool,
    content_start: usize,
    first: usize,
    last: usize,
    overflow: bool,
}

fn visible_thought(
    thought: &scroll::ThoughtRows,
    offset: usize,
    viewport_end: usize,
    board: Rect,
) -> Option<ThoughtLayout> {
    let visible = visible_rows(thought, offset, viewport_end)?;
    let area = Rect::new(
        board.x,
        viewport_y(board, visible.first, offset),
        board.width,
        u16::try_from(visible.last.saturating_sub(visible.first)).unwrap_or(u16::MAX),
    );
    let text_area = Rect::new(
        area.x.saturating_add(2).min(area.right()),
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );
    let overflow = thought
        .overflow_row
        .filter(|_| visible.overflow)
        .map(|row| {
            Rect::new(
                text_area.x,
                viewport_y(board, row, offset),
                text_area.width,
                1,
            )
        });
    let viewport_clipped = thought.content_start < offset || thought.end > viewport_end;
    Some(ThoughtLayout {
        thought_id: thought.thought_id,
        index: thought.index,
        separator_before: visible.separator.then(|| separator(thought, offset, board)),
        area,
        text_area,
        gutter: Rect::new(area.x, area.y, area.width.min(1), area.height),
        overflow,
        hidden_rows: thought.overflow_row.map_or(0, |_| {
            thought.natural_rows.saturating_sub(thought.content_rows)
        }),
        viewport_clipped,
        scrollable_hidden: viewport_clipped
            && thought.presentation != ThoughtPresentation::Collapsed,
        content_row_offset: visible
            .content_start
            .saturating_sub(thought.content_start)
            .min(thought.content_rows),
    })
}

fn visible_rows(
    thought: &scroll::ThoughtRows,
    offset: usize,
    viewport_end: usize,
) -> Option<VisibleRows> {
    let content_end = thought.content_start.saturating_add(thought.content_rows);
    let content_start = thought.content_start.max(offset);
    let content_end = content_end.min(viewport_end);
    let content = content_start < content_end;
    let overflow = thought
        .overflow_row
        .is_some_and(|row| row >= offset && row < viewport_end);
    let gap = thought.gap_start < viewport_end && thought.content_start > offset;
    if !gap && !content && !overflow {
        return None;
    }
    let first = if content {
        content_start
    } else {
        thought
            .overflow_row
            .filter(|_| overflow)
            .unwrap_or(thought.content_start.min(viewport_end))
    };
    let last = if overflow {
        thought.overflow_row.unwrap_or(first).saturating_add(1)
    } else if content {
        content_end
    } else {
        first
    };
    Some(VisibleRows {
        separator: thought.gap_rows > 0
            && thought.gap_start >= offset
            && thought.gap_start < viewport_end,
        content_start,
        first,
        last,
        overflow,
    })
}

fn separator(thought: &scroll::ThoughtRows, offset: usize, board: Rect) -> Rect {
    Rect::new(
        board.x.saturating_add(2).min(board.right()),
        viewport_y(board, thought.gap_start, offset),
        board.width.saturating_sub(2),
        1,
    )
}

fn visible_row(row: usize, offset: usize, height: u16) -> bool {
    row >= offset && row < offset.saturating_add(usize::from(height))
}

fn viewport_y(board: Rect, row: usize, offset: usize) -> u16 {
    board
        .y
        .saturating_add(u16::try_from(row.saturating_sub(offset)).unwrap_or(u16::MAX))
}
