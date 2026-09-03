//! Canonical contextual-help entries and responsive grid measurements.

use crate::application::InteractionMode;

use super::shortcut_metadata::{self, ShortcutAction};
use super::{BoardApp, UiKey};

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
        (
            shortcut_metadata::board_label(ShortcutAction::Copy, keys),
            "Copy",
        ),
        (
            shortcut_metadata::board_label(ShortcutAction::Cut, keys),
            "Cut",
        ),
        (keys.delete_label(), "Delete"),
        (
            shortcut_metadata::primary_label(ShortcutAction::Duplicate),
            "Duplicate",
        ),
        (super::settings::key_label(keys.select), "Select"),
        (
            format!(
                "{}/{}",
                shortcut_metadata::primary_label(ShortcutAction::SelectAll),
                super::settings::key_label(keys.select_all)
            ),
            "Select all",
        ),
        (super::settings::key_label(keys.range_select), "Latch"),
        (
            shortcut_metadata::board_label(ShortcutAction::Undo, keys),
            "Undo",
        ),
        (
            shortcut_metadata::primary_label(ShortcutAction::Paste),
            "Paste",
        ),
        (shortcut_metadata::redo_label(), "Redo"),
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
    items.push((
        shortcut_metadata::board_label(ShortcutAction::Quit, keys),
        "Quit",
    ));
    items.push((help_close_label(keys), "Close"));
    items
}

fn edit_items(
    mode: InteractionMode,
    keys: &super::settings::KeyBindings,
    supports_submission: bool,
) -> Vec<Shortcut> {
    let mut items = vec![
        (help_close_label(keys), "Close"),
        (
            shortcut_metadata::primary_label(ShortcutAction::Copy),
            "Copy",
        ),
        (shortcut_metadata::primary_label(ShortcutAction::Cut), "Cut"),
        (
            shortcut_metadata::primary_label(ShortcutAction::Paste),
            "Paste",
        ),
        (
            shortcut_metadata::primary_label(ShortcutAction::SelectAll),
            "Select all",
        ),
        (
            shortcut_metadata::primary_label(ShortcutAction::DeleteLogicalLine),
            "Delete logical line",
        ),
        (
            primary(&format!("Shift+{}", keys.delete_sentence)),
            "Delete sentence",
        ),
        (
            shortcut_metadata::primary_label(ShortcutAction::Undo),
            "Undo",
        ),
        (shortcut_metadata::redo_label(), "Redo"),
        (
            primary(&transform_key_label(keys.transform)),
            "Split/extract",
        ),
        (
            super::paging::FAST_NAVIGATION_SHORTCUT_KEY.to_owned(),
            super::paging::FAST_NAVIGATION_SHORTCUT_LABEL,
        ),
        (format!("{}/{}", primary("↑"), primary("↓")), "Start/end"),
        visual_row_selection_shortcut(keys),
        ("↑/↓×2".to_owned(), "Neighbor/new"),
    ];
    if supports_submission {
        let [remove, keep] = submission_items(mode, keys);
        items.insert(1, remove);
        items.insert(2, keep);
    }
    items
}

fn visual_row_selection_shortcut(keys: &super::settings::KeyBindings) -> Shortcut {
    visual_row_selection_shortcut_for_platform(keys, cfg!(target_os = "macos"))
}

fn visual_row_selection_shortcut_for_platform(
    keys: &super::settings::KeyBindings,
    macos: bool,
) -> Shortcut {
    let fallback = format!(
        "{}/{}",
        keys.select_visual_row_start, keys.select_visual_row_end
    );
    let suffix = if macos {
        format!("Shift+←/→/{fallback}")
    } else {
        format!("Shift+{fallback}")
    };
    (primary(&suffix), "Select visual row")
}

fn help_close_label(keys: &super::settings::KeyBindings) -> String {
    let configured = if keys.help == ' ' {
        UiKey::UnmodifiedSpace
    } else {
        UiKey::Character(keys.help)
    };
    if configured.list_navigation().is_some() {
        "Esc".to_owned()
    } else {
        format!("Esc/{}", super::settings::key_label(keys.help))
    }
}

fn submission_items(mode: InteractionMode, keys: &crate::ui::KeyBindings) -> [Shortcut; 2] {
    let key = |action| {
        if matches!(mode, InteractionMode::Board) {
            shortcut_metadata::board_label(action, keys)
        } else {
            shortcut_metadata::canonical_label(action)
        }
    };
    [
        (key(ShortcutAction::Submit), "Submit"),
        (key(ShortcutAction::SubmitKeep), "Submit & keep"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_row_help_only_advertises_primary_arrows_on_macos() {
        let keys = super::super::settings::KeyBindings::default();
        let prefix = if cfg!(target_os = "macos") {
            "Command+"
        } else {
            "Ctrl+"
        };
        assert_eq!(
            visual_row_selection_shortcut_for_platform(&keys, true),
            (format!("{prefix}Shift+←/→/H/L"), "Select visual row")
        );
        assert_eq!(
            visual_row_selection_shortcut_for_platform(&keys, false),
            (format!("{prefix}Shift+H/L"), "Select visual row")
        );
    }

    #[test]
    fn help_close_label_derives_from_modal_navigation_precedence() {
        let mut keys = super::super::settings::KeyBindings::default();
        assert_eq!(help_close_label(&keys), "Esc/?");

        keys.help = 'j';
        assert_eq!(help_close_label(&keys), "Esc");

        keys.help = ' ';
        assert_eq!(help_close_label(&keys), "Esc/Space");
    }
}
