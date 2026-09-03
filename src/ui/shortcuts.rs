//! Canonical contextual-help entries and responsive grid measurements.

use crate::application::InteractionMode;

use super::BoardApp;

pub(crate) type Shortcut = (String, &'static str);

pub(crate) fn items(app: &BoardApp) -> Vec<Shortcut> {
    let keys = app.keybindings();
    if matches!(
        app.interaction_mode(),
        InteractionMode::Compose | InteractionMode::Edit { .. }
    ) {
        return edit_items(app.interaction_mode(), keys, app.supports_submission());
    }
    let mut items = vec![
        (keys.new.to_string(), "New"),
        (format!("Enter/{}", keys.edit), "Edit"),
        (
            format!("{}/↓ {}/↑", keys.focus_down, keys.focus_up),
            "Move/new×2",
        ),
        (format!("{}/{}", keys.range_down, keys.range_up), "Range"),
        (
            primary(&format!("{}/{}", keys.range_down, keys.range_up)),
            "Reorder",
        ),
        (keys.copy.to_string(), "Copy"),
        (keys.cut.to_string(), "Cut"),
        (keys.delete_label(), "Delete"),
        (primary("D"), "Duplicate"),
        (super::settings::key_label(keys.select), "Select"),
        (
            format!(
                "{}/{}",
                super::settings::key_label(keys.select_all),
                primary("A")
            ),
            "Select all",
        ),
        (super::settings::key_label(keys.range_select), "Latch"),
        (keys.undo.to_string(), "Undo"),
        (super::settings::key_label(keys.collapse), "Collapse"),
        (keys.search.to_string(), "Search"),
        (keys.commands.to_string(), "Commands"),
        (keys.screenshot_inbox.to_string(), "Inbox"),
    ];
    if matches!(
        keys.command(keys.transform),
        Some(super::settings::BoardCommand::Transform)
    ) {
        items.insert(10, (keys.transform.to_string(), "Transform"));
    }
    if app.supports_submission() {
        items.extend(submission_items(InteractionMode::Board, keys));
    }
    items.push((keys.quit.to_string(), "Quit"));
    items.push((keys.help.to_string(), "Close"));
    items
}

fn edit_items(
    mode: InteractionMode,
    keys: &super::settings::KeyBindings,
    supports_submission: bool,
) -> Vec<Shortcut> {
    let mut items = vec![
        ("Esc".to_owned(), "Board"),
        (primary("C"), "Copy"),
        (primary("X"), "Cut"),
        (primary("A"), "Select all"),
        (primary("U"), "Delete logical line"),
        (
            primary(&format!("Shift+{}", keys.delete_sentence)),
            "Delete sentence",
        ),
        (primary("Z"), "Undo"),
        (primary("Shift+Z"), "Redo"),
        (
            primary(&transform_key_label(keys.transform)),
            "Split/extract",
        ),
        (
            super::paging::FAST_NAVIGATION_SHORTCUT_KEY.to_owned(),
            super::paging::FAST_NAVIGATION_SHORTCUT_LABEL,
        ),
        (format!("{}/{}", primary("↑"), primary("↓")), "Start/end"),
        (
            format!(
                "←/→·{}/{}",
                keys.select_visual_row_start, keys.select_visual_row_end
            ),
            "Primary+Shift row",
        ),
        ("↑/↓×2".to_owned(), "Neighbor/new"),
    ];
    if supports_submission {
        let [remove, keep] = submission_items(mode, keys);
        items.insert(1, remove);
        items.insert(2, keep);
    }
    items
}

fn submission_items(mode: InteractionMode, keys: &crate::ui::KeyBindings) -> [Shortcut; 2] {
    let board_key = |key: char, suffix: &str| {
        if matches!(mode, InteractionMode::Board) {
            format!("{}/{}", super::settings::key_label(key), primary(suffix))
        } else {
            primary(suffix)
        }
    };
    [
        (board_key(keys.submit_remove, "Enter"), "Submit"),
        (board_key(keys.submit_keep, "Shift+Enter"), "Submit & keep"),
    ]
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
    super::settings::primary_key_label(suffix)
}

fn transform_key_label(key: char) -> String {
    if key.is_ascii_alphabetic() {
        key.to_ascii_uppercase().to_string()
    } else {
        key.to_string()
    }
}
