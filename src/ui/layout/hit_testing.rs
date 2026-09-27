//! Hit resolution over the exact rectangles prepared for rendering.

use super::{HitTarget, LayoutSnapshot};

impl LayoutSnapshot {
    /// Resolve one terminal cell through the same rectangles used to render.
    #[must_use]
    pub fn hit_test(&self, column: u16, row: u16) -> Option<HitTarget> {
        if let Some(overlay) = &self.overlay {
            if crate::ui::geometry::contains(overlay.close, column, row) {
                return Some(HitTarget::CloseOverlay);
            }
            return overlay.items.iter().enumerate().find_map(|(index, area)| {
                (overlay
                    .item_interactive
                    .get(index)
                    .copied()
                    .unwrap_or(false)
                    && crate::ui::geometry::contains(*area, column, row))
                .then_some(HitTarget::PaletteItem(index))
            });
        }
        for thought in &self.thoughts {
            if thought
                .name
                .is_some_and(|area| crate::ui::geometry::contains(area, column, row))
            {
                return Some(HitTarget::ThoughtName(thought.thought_id));
            }
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
        for separator in &self.separators {
            if crate::ui::geometry::contains(separator.gutter, column, row) {
                return Some(HitTarget::SeparatorDragHandle(separator.separator_id));
            }
            if crate::ui::geometry::contains(separator.area, column, row) {
                return Some(HitTarget::Separator(separator.separator_id));
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
}
