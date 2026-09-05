//! Contextual Help projected from ordered descriptor metadata.

use crate::{
    application::InteractionMode,
    ui::{BoardApp, KeyBindings, ShortcutActionId as Action},
};

use super::{board_label, canonical_label, primary_label, redo_label, shifted_primary_label};
use crate::ui::shortcut_registry::{HelpAvailability, HelpSurface};

pub(crate) type HelpItem = (String, &'static str);

pub(crate) fn help_items(app: &BoardApp) -> Vec<HelpItem> {
    let mode = app.interaction_mode();
    let surface = if matches!(
        mode,
        InteractionMode::Compose | InteractionMode::Edit { .. }
    ) {
        HelpSurface::Editor
    } else {
        HelpSurface::Board
    };
    app.shortcut_registry()
        .help(surface)
        .into_iter()
        .filter(|(action, metadata)| available(app, *action, metadata.availability))
        .map(|(action, metadata)| (key_label(action, mode, app.keybindings()), metadata.label))
        .collect()
}

fn available(app: &BoardApp, action: Action, availability: HelpAvailability) -> bool {
    match availability {
        HelpAvailability::Always => true,
        HelpAvailability::Submission => app.supports_submission(),
        HelpAvailability::EffectiveTransform => {
            app.board_shortcut_action(app.keybindings().transform) == Some(action)
        }
    }
}

fn key_label(action: Action, mode: InteractionMode, keys: &KeyBindings) -> String {
    if matches!(
        mode,
        InteractionMode::Compose | InteractionMode::Edit { .. }
    ) {
        editor_key_label(action, mode, keys)
    } else {
        board_key_label(action, keys)
    }
}

fn board_key_label(action: Action, keys: &KeyBindings) -> String {
    match action {
        Action::New => keys.new.to_string(),
        Action::Edit => format!("Enter/{}", keys.edit),
        Action::FocusNext => format!("{}/↓ {}/↑", keys.focus_down, keys.focus_up),
        Action::ExtendNext => format!("{}/{}", keys.range_down, keys.range_up),
        Action::MoveDown => primary(&format!("{}/{}", keys.range_down, keys.range_up)),
        Action::Copy
        | Action::Cut
        | Action::Undo
        | Action::PasteExact
        | Action::PasteReflow
        | Action::SubmitRemove
        | Action::SubmitKeep
        | Action::Quit => board_label(action, keys),
        Action::Delete => keys.delete_label(),
        Action::Duplicate => primary_label(action),
        Action::Select => crate::ui::settings::key_label(keys.select),
        Action::ContextualTransform => keys.transform.to_string(),
        Action::SelectAll => format!(
            "{}/{}",
            primary_label(action),
            crate::ui::settings::key_label(keys.select_all)
        ),
        Action::RangeSelect => crate::ui::settings::key_label(keys.range_select),
        Action::Redo => redo_label(),
        Action::Collapse => crate::ui::settings::key_label(keys.collapse),
        Action::OpenSearch => keys.search.to_string(),
        Action::OpenCommands => keys.commands.to_string(),
        Action::ScreenshotInbox => keys.screenshot_inbox.to_string(),
        Action::Close => "Esc".to_owned(),
        _ => canonical_label(action),
    }
}

fn editor_key_label(action: Action, mode: InteractionMode, keys: &KeyBindings) -> String {
    match action {
        Action::Close => "Esc".to_owned(),
        Action::SubmitRemove | Action::SubmitKeep => {
            if matches!(mode, InteractionMode::Board) {
                board_label(action, keys)
            } else {
                canonical_label(action)
            }
        }
        Action::Copy
        | Action::Cut
        | Action::PasteExact
        | Action::SelectAll
        | Action::DeleteLogicalLine
        | Action::Undo => primary_label(action),
        Action::PasteReflow => shifted_primary_label(action),
        Action::DeleteSentence => primary(&format!("Shift+{}", keys.delete_sentence)),
        Action::Redo => redo_label(),
        Action::ContextualTransform => primary(&transform_key_label(keys.transform)),
        Action::FastNext => crate::ui::paging::FAST_NAVIGATION_SHORTCUT_KEY.to_owned(),
        Action::MoveDocumentStart => format!("{}/{}", primary("↑"), primary("↓")),
        Action::ExtendVisualRowStart => {
            visual_row_selection_shortcut(keys, cfg!(target_os = "macos"))
        }
        Action::MoveVisualDown => "↑/↓×2".to_owned(),
        _ => canonical_label(action),
    }
}

fn visual_row_selection_shortcut(keys: &KeyBindings, macos: bool) -> String {
    let fallback = format!(
        "{}/{}",
        keys.select_visual_row_start, keys.select_visual_row_end
    );
    let suffix = if macos {
        format!("Shift+←/→/{fallback}")
    } else {
        format!("Shift+{fallback}")
    };
    primary(&suffix)
}

fn primary(suffix: &str) -> String {
    crate::ui::settings::primary_key_label(suffix)
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
        let keys = KeyBindings::default();
        let prefix = if cfg!(target_os = "macos") {
            "Cmd+"
        } else {
            "Ctrl+"
        };
        assert_eq!(
            visual_row_selection_shortcut(&keys, true),
            format!("{prefix}Shift+←/→/H/L")
        );
        assert_eq!(
            visual_row_selection_shortcut(&keys, false),
            format!("{prefix}Shift+H/L")
        );
    }

    #[test]
    fn board_help_lists_only_effective_paste_pair_fallbacks() {
        let mut keys = KeyBindings::default();
        assert_eq!(
            board_label(Action::PasteExact, &keys),
            format!("{}/p", primary("V"))
        );
        assert_eq!(
            board_label(Action::PasteReflow, &keys),
            format!("{}/P", primary("Shift+V"))
        );

        keys.paste = 'g';
        keys.submit_keep = 'G';
        assert_eq!(
            board_label(Action::PasteExact, &keys),
            format!("{}/g", primary("V"))
        );
        assert_eq!(board_label(Action::PasteReflow, &keys), primary("Shift+V"));
    }
}
