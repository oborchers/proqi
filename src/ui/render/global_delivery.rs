//! Typed global delivery picker projection.

use ratatui_core::terminal::Frame;

use crate::ui::{BoardApp, Theme, layout::OverlayLayout};

use super::overlays;

pub(super) fn render(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    app: &BoardApp,
    picker: &crate::ui::app::global_delivery::GlobalDeliveryView,
    theme: &Theme,
) {
    let rows = picker
        .choices
        .iter()
        .map(|choice| {
            overlays::PickerRow::choice(&choice.primary, &choice.secondary, choice.enabled)
        })
        .collect::<Vec<_>>();
    overlays::render_picker(
        frame,
        overlay,
        overlays::PickerView {
            title: picker.title,
            prompt: '›',
            query: &picker.query,
            cursor: app.overlay_query_cursor().unwrap_or(picker.query.len()),
            entries: &rows,
            selected: picker.selected,
        },
        app.picker_overflow(overlay.items.len()),
        theme,
    );
}
