//! Focus-owned neighbors within one tall expanded thought presentation.

use super::{BoardFlow, ScrollAnchor, ScrollGeometry};
use crate::domain::ThoughtId;

impl ScrollGeometry {
    pub(in crate::ui) fn neighbor(self, delta: isize) -> Option<ScrollAnchor> {
        if delta > 0 { self.next } else { self.previous }
    }

    pub(in crate::ui) fn focused_neighbor(self, delta: isize) -> Option<ScrollAnchor> {
        if delta > 0 {
            self.focused_next
        } else {
            self.focused_previous
        }
    }
}

pub(super) fn neighbors(
    flow: &BoardFlow,
    offset: usize,
    viewport_height: usize,
    maximum: usize,
    focused: Option<ThoughtId>,
) -> (Option<ScrollAnchor>, Option<ScrollAnchor>) {
    let Some(rows) = focused.and_then(|id| flow.thought(id)) else {
        return (None, None);
    };
    let viewport_end = offset.saturating_add(viewport_height);
    let visible = rows.end > offset && rows.content_start < viewport_end;
    if !visible
        || rows.presentation != crate::domain::ThoughtPresentation::Expanded
        || rows.content_rows <= viewport_height
    {
        return (None, None);
    }
    let previous = (rows.content_start < offset && offset > 0)
        .then(|| flow.anchor_at(offset.saturating_sub(1)));
    let next = (rows.end > viewport_end && offset < maximum)
        .then(|| flow.anchor_at(offset.saturating_add(1)));
    (previous, next)
}
