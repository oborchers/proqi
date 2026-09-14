//! Modal geometry shared by rendering, scrolling, and pointer hit testing.

use ratatui_core::layout::Rect;

use super::{LayoutSnapshot, controls};

/// Geometry for a centered modal overlay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverlayLayout {
    /// Complete bordered overlay.
    pub area: Rect,
    /// Visible command rows.
    pub items: Vec<Rect>,
    /// Optional passive heading row immediately above each visible item.
    pub item_headings: Vec<Option<Rect>>,
    /// Whether each rendered item rectangle accepts pointer activation.
    pub item_interactive: Vec<bool>,
    /// Stable close target in the upper-right corner.
    pub close: Rect,
}

impl LayoutSnapshot {
    /// Attach modal geometry after application overlays are known.
    pub fn configure_overlay(&mut self, item_count: usize, preferred_rows: usize) {
        self.overlay = (preferred_rows > 0).then(|| {
            let required = controls::overlay_height(preferred_rows);
            let covers_chrome = self.board.height < required;
            let bounds = if covers_chrome { self.area } else { self.board };
            controls::overlay_layout(bounds, item_count, preferred_rows, covers_chrome)
        });
    }

    /// Attach invocation geometry with passive headings separate from item hit targets.
    pub fn configure_grouped_overlay(&mut self, item_groups: &[bool], preferred_rows: usize) {
        self.overlay = (preferred_rows > 0).then(|| {
            let required = controls::overlay_height(preferred_rows);
            let covers_chrome = self.board.height < required;
            let bounds = if covers_chrome { self.area } else { self.board };
            controls::grouped_overlay_layout(bounds, item_groups, preferred_rows, covers_chrome)
        });
    }

    /// Attach Commands geometry whose disabled rows and headings are passive.
    pub(crate) fn configure_command_overlay(
        &mut self,
        item_groups: &[bool],
        item_interactive: &[bool],
        preferred_rows: usize,
    ) {
        self.overlay = (preferred_rows > 0).then(|| {
            let required = controls::overlay_height(preferred_rows);
            let covers_chrome = self.board.height < required;
            let bounds = if covers_chrome { self.area } else { self.board };
            controls::grouped_overlay_layout_with_interactivity(
                bounds,
                item_groups,
                item_interactive,
                preferred_rows,
                covers_chrome,
            )
        });
    }
}
