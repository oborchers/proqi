//! Shared terminal-cell geometry used by rendering and pointer hit testing.

use ratatui_core::layout::Rect;

pub(crate) const fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}

pub(crate) const fn inset_horizontal(area: Rect, cells: u16) -> Rect {
    Rect::new(
        area.x.saturating_add(cells),
        area.y,
        area.width.saturating_sub(cells.saturating_mul(2)),
        area.height,
    )
}

pub(crate) fn row(area: Rect, offset: u16) -> Rect {
    Rect::new(
        area.x,
        area.y.saturating_add(offset),
        area.width,
        u16::from(offset < area.height),
    )
}
