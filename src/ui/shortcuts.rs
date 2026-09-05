//! Responsive measurements for registry-projected contextual Help.

use super::{BoardApp, shortcut_registry::presentation};

pub(crate) type Shortcut = presentation::HelpItem;

pub(crate) fn items(app: &BoardApp) -> Vec<Shortcut> {
    presentation::help_items(app)
}

pub(crate) fn grid_metrics(items: &[Shortcut], width: u16) -> (usize, usize) {
    let key_width = items
        .iter()
        .map(|(key, _)| crate::ports::text_layout::terminal_cell_width(key))
        .max()
        .unwrap_or(1);
    let widest = items
        .iter()
        .map(|(_, label)| key_width + 1 + crate::ports::text_layout::terminal_cell_width(label))
        .max()
        .unwrap_or(1);
    let columns = if usize::from(width) >= widest.saturating_mul(2) {
        2
    } else {
        1
    };
    (columns, key_width)
}

pub(crate) fn row_count(app: &BoardApp, width: u16) -> usize {
    let items = items(app);
    let (columns, _) = grid_metrics(&items, width);
    items.len().div_ceil(columns)
}
