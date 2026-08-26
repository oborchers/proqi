//! Responsive thought placement and line-level board scrolling.

use ratatui_core::layout::Rect;

use crate::{
    application::AppState,
    domain::{Thought, ThoughtPresentation},
    ports::{editor::EditorSnapshot, text_layout::wrap_rows},
};

use super::{ThoughtLayout, scroll};

pub(super) struct ContentRequest<'a> {
    pub(super) state: &'a AppState,
    pub(super) editor: Option<&'a EditorSnapshot>,
    pub(super) board: Rect,
    pub(super) requested_first: usize,
    pub(super) content_width: u16,
    pub(super) insertion_focused: bool,
    pub(super) density: crate::ui::settings::BoardDensity,
    pub(super) requested_row_offset: usize,
}

pub(super) struct VisibleContent {
    pub(super) thoughts: Vec<ThoughtLayout>,
    pub(super) insert: Option<Rect>,
    pub(super) first: usize,
    pub(super) first_row_offset: usize,
    pub(super) max_first: usize,
}

pub(super) fn visible_content(request: &ContentRequest<'_>) -> VisibleContent {
    let live = request.state.board.live_thoughts();
    let focus_index = request
        .state
        .focused_thought
        .and_then(|id| live.iter().position(|thought| thought.id == id));
    let board_mode = matches!(
        request.state.mode,
        crate::application::InteractionMode::Board
    );
    let max_first = scroll::maximum_first(
        request.state,
        request.editor,
        request.board,
        request.content_width,
        board_mode,
        request.density,
    );
    let mut first = request.requested_first.min(max_first);
    if request.insertion_focused {
        first = max_first;
    } else if focus_index.is_some_and(|index| index < first) {
        first = focus_index.unwrap_or(first).min(max_first);
    }
    let row_offset = if request.insertion_focused || focus_index.is_some() {
        0
    } else {
        request.requested_row_offset
    };
    let mut thoughts = thoughts_from(request, first, max_first, board_mode, row_offset);
    if focus_index.is_some_and(|index| !thoughts.iter().any(|layout| layout.index == index)) {
        first = focus_index.unwrap_or(first).min(max_first);
        thoughts = thoughts_from(request, first, max_first, board_mode, 0);
    }
    let used_bottom = thoughts
        .last()
        .map_or(request.board.y, |layout| layout.area.bottom());
    let insert_space = request.board.bottom().saturating_sub(used_bottom);
    let insert = (board_mode && first == max_first && insert_space > 0).then(|| {
        let y = used_bottom.saturating_add(u16::from(insert_space >= 2));
        Rect::new(request.board.x, y, request.board.width, 1)
    });
    let first_row_offset = thoughts
        .first()
        .map_or(0, |layout| layout.content_row_offset);
    VisibleContent {
        thoughts,
        insert,
        first,
        first_row_offset,
        max_first,
    }
}

fn thoughts_from(
    request: &ContentRequest<'_>,
    first: usize,
    max_first: usize,
    board_mode: bool,
    row_offset: usize,
) -> Vec<ThoughtLayout> {
    let board = scroll::board_for_page(request.board, board_mode && first == max_first);
    place_thoughts(
        &ThoughtPlacement {
            state: request.state,
            editor: request.editor,
            board,
            content_width: request.content_width,
            density: request.density,
        },
        first,
        row_offset,
    )
}

pub(super) struct ThoughtPlacement<'a> {
    pub(super) state: &'a AppState,
    pub(super) editor: Option<&'a EditorSnapshot>,
    pub(super) board: Rect,
    pub(super) content_width: u16,
    pub(super) density: crate::ui::settings::BoardDensity,
}

pub(super) fn place_thoughts(
    context: &ThoughtPlacement<'_>,
    first: usize,
    requested_row_offset: usize,
) -> Vec<ThoughtLayout> {
    let mut layouts = Vec::new();
    let live = context.state.board.live_thoughts();
    let remaining = live.len().saturating_sub(first);
    let roomy = usize::from(context.board.height)
        >= remaining.saturating_add(remaining.saturating_sub(1).saturating_mul(3));
    let top_padding = u16::from(roomy && context.board.height >= 3 && remaining > 0);
    let mut y = context.board.y.saturating_add(top_padding);
    for (index, thought) in live.into_iter().enumerate().skip(first) {
        if y >= context.board.bottom() {
            break;
        }
        let comfortable =
            roomy && context.density == crate::ui::settings::BoardDensity::Comfortable;
        let separation = if comfortable { 2 } else { 1 };
        if !layouts.is_empty() && y.saturating_add(separation) >= context.board.bottom() {
            break;
        }
        let separator_before =
            separator_before(context.board, &mut y, !layouts.is_empty(), comfortable);
        let layout = place_thought(
            context,
            thought,
            index,
            first,
            y,
            separator_before,
            requested_row_offset,
        );
        y = layout.area.bottom();
        layouts.push(layout);
    }
    layouts
}

fn place_thought(
    context: &ThoughtPlacement<'_>,
    thought: &Thought,
    index: usize,
    first: usize,
    y: u16,
    separator_before: Option<Rect>,
    requested_row_offset: usize,
) -> ThoughtLayout {
    let editing = context
        .editor
        .is_some_and(|_| context.state.focused_thought == Some(thought.id));
    let natural = context
        .editor
        .filter(|_| context.state.focused_thought == Some(thought.id))
        .map_or_else(
            || wrapped_rows(&thought.content, context.content_width),
            |snapshot| snapshot.visual_lines.len().max(1),
        );
    let content_row_offset =
        if index == first && !editing && thought.presentation != ThoughtPresentation::Collapsed {
            requested_row_offset.min(natural.saturating_sub(1))
        } else {
            0
        };
    let explicit_cap = match thought.presentation {
        ThoughtPresentation::Expanded => natural,
        ThoughtPresentation::Collapsed => 2,
        ThoughtPresentation::Automatic => usize::from(responsive_cap(context.board.height)),
    };
    let available = usize::from(context.board.bottom().saturating_sub(y));
    let desired = natural
        .saturating_sub(content_row_offset)
        .min(explicit_cap.max(1));
    let viewport_clipped = desired > available;
    let height = u16::try_from(desired.min(available).max(1)).unwrap_or(u16::MAX);
    let area = Rect::new(context.board.x, y, context.board.width, height);
    let gutter = Rect::new(area.x, area.y, area.width.min(1), area.height);
    let text_area = Rect::new(
        area.x.saturating_add(2).min(area.right()),
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );
    let remaining_rows = natural.saturating_sub(content_row_offset);
    let hidden_rows = if editing || remaining_rows <= usize::from(height) {
        0
    } else {
        remaining_rows.saturating_sub(usize::from(height).saturating_sub(1))
    };
    let scrollable_hidden =
        hidden_rows > 0 && thought.presentation != ThoughtPresentation::Collapsed;
    let overflow = (hidden_rows > 0).then(|| {
        Rect::new(
            text_area.x,
            area.bottom().saturating_sub(1),
            text_area.width,
            1,
        )
    });
    ThoughtLayout {
        thought_id: thought.id,
        index,
        separator_before,
        area,
        text_area,
        gutter,
        overflow,
        hidden_rows,
        viewport_clipped,
        scrollable_hidden,
        content_row_offset,
    }
}

fn separator_before(board: Rect, y: &mut u16, preceded: bool, roomy: bool) -> Option<Rect> {
    preceded.then(|| {
        let rule_y = *y;
        let separator = Rect::new(
            board.x.saturating_add(2).min(board.right()),
            rule_y,
            board.width.saturating_sub(2),
            1,
        );
        *y = y.saturating_add(if roomy { 2 } else { 1 });
        separator
    })
}

fn responsive_cap(board_height: u16) -> u16 {
    board_height.saturating_mul(2).div_ceil(3).max(3)
}

fn wrapped_rows(content: &str, width: u16) -> usize {
    wrap_rows(content, usize::from(width.max(1))).len().max(1)
}
