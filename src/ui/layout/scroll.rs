//! Board scroll bounds derived from authoritative thought placement.

use std::collections::BTreeSet;

use ratatui_core::layout::Rect;

use crate::{application::AppState, domain::ThoughtId, ports::editor::EditorSnapshot};

pub(super) fn board_for_page(board: Rect, reserve_insert: bool) -> Rect {
    if reserve_insert {
        Rect::new(
            board.x,
            board.y,
            board.width,
            board.height.saturating_sub(1),
        )
    } else {
        board
    }
}

pub(super) fn maximum_first(
    state: &AppState,
    editor: Option<&EditorSnapshot>,
    board: Rect,
    content_width: u16,
    expanded: &BTreeSet<ThoughtId>,
    include_insert: bool,
    density: crate::ui::settings::BoardDensity,
) -> usize {
    let live_count = state.board.live_thoughts().len();
    if live_count == 0 || board.height == 0 {
        return 0;
    }
    let thought_board = if include_insert {
        Rect::new(
            board.x,
            board.y,
            board.width,
            board.height.saturating_sub(1),
        )
    } else {
        board
    };
    for candidate in 0..live_count {
        let layouts = super::content::place_thoughts(
            &super::content::ThoughtPlacement {
                state,
                editor,
                board: thought_board,
                content_width,
                expanded,
                density,
            },
            candidate,
            0,
        );
        let reaches_end = layouts
            .last()
            .is_some_and(|layout| layout.index + 1 == live_count && !layout.viewport_clipped);
        if reaches_end {
            return candidate;
        }
    }
    if include_insert {
        live_count
    } else {
        live_count.saturating_sub(1)
    }
}
