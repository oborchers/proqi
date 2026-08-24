//! Board scroll bounds derived from authoritative thought placement.

use std::collections::BTreeSet;

use ratatui_core::layout::Rect;

use crate::{application::AppState, domain::ThoughtId, ports::editor::EditorSnapshot};

pub(super) fn maximum_first(
    state: &AppState,
    editor: Option<&EditorSnapshot>,
    board: Rect,
    content_width: u16,
    expanded: &BTreeSet<ThoughtId>,
) -> usize {
    let live_count = state.board.live_thoughts().len();
    if live_count < 2 || board.height == 0 {
        return 0;
    }
    for candidate in 0..live_count {
        let layouts =
            super::place_thoughts(state, editor, board, content_width, candidate, expanded);
        let reaches_end = layouts
            .last()
            .is_some_and(|layout| layout.index + 1 == live_count);
        let leaves_insert_row = layouts
            .last()
            .is_none_or(|layout| layout.area.bottom() < board.bottom());
        if reaches_end && (candidate > 0 || leaves_insert_row) {
            return candidate;
        }
    }
    live_count.saturating_sub(1)
}
