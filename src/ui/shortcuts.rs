//! Canonical contextual-help entries and responsive grid measurements.

use crate::application::InteractionMode;

use super::BoardApp;

pub(crate) type Shortcut = (String, &'static str);

pub(crate) fn items(app: &BoardApp) -> Vec<Shortcut> {
    let keys = app.keybindings();
    if matches!(app.interaction_mode(), InteractionMode::Edit { .. }) {
        return vec![
            ("Esc".to_owned(), "Board"),
            (primary("A"), "Select all"),
            (primary("U"), "Delete line"),
            (primary("Z"), "Undo"),
            (primary("Shift+Z"), "Redo"),
            ("Alt+↑/↓".to_owned(), "Jump 5 rows"),
            (format!("{}/{}", primary("↑"), primary("↓")), "Start/end"),
            (keys.commands.to_string(), "Commands"),
            (keys.help.to_string(), "Close"),
        ];
    }
    let mut items = vec![
        (keys.new.to_string(), "New"),
        (format!("Enter/{}", keys.edit), "Edit"),
        (format!("{}/{}", keys.focus_down, keys.focus_up), "Move"),
        (format!("{}/{}", keys.range_down, keys.range_up), "Range"),
        (
            primary(&format!("{}/{}", keys.range_down, keys.range_up)),
            "Reorder",
        ),
        (keys.copy.to_string(), "Copy"),
        (keys.cut.to_string(), "Cut"),
        (keys.delete.to_string(), "Delete"),
        (primary("D"), "Duplicate"),
        (super::settings::key_label(keys.select), "Select"),
        (super::settings::key_label(keys.range_select), "Latch"),
        (keys.undo.to_string(), "Undo"),
        (super::settings::key_label(keys.collapse), "Collapse"),
        (keys.search.to_string(), "Search"),
        (keys.commands.to_string(), "Commands"),
        (keys.help.to_string(), "Close"),
    ];
    if app.supports_submission() {
        items.push((keys.submit_remove.to_string(), "Submit"));
        items.push((keys.submit_keep.to_string(), "Submit & keep"));
    }
    items.push((keys.quit.to_string(), "Quit"));
    items
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

fn primary(suffix: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{suffix}")
    } else {
        format!("Ctrl+{suffix}")
    }
}
