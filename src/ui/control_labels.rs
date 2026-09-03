//! Canonical visible labels and terminal-cell widths for clickable controls.

use crate::{
    application::InteractionMode,
    domain::Direction,
    ports::agent::{AgentTarget, SubmissionDisposition},
};

use super::shortcut_metadata::{self, ShortcutAction};
use super::{HitTarget, KeyBindings};

pub(crate) struct ControlLabel {
    pub(crate) key: String,
    pub(crate) text: String,
}

pub(crate) const fn insertion_text(mode: InteractionMode, compact: bool) -> &'static str {
    match (mode, compact) {
        (InteractionMode::Compose, true) => " Type",
        (InteractionMode::Compose, false) => " Start typing",
        (_, true) => " New",
        (_, false) => " New thought",
    }
}

impl ControlLabel {
    pub(crate) fn width(&self) -> u16 {
        u16::try_from(
            crate::ports::text_layout::terminal_cell_width(&self.key)
                .saturating_add(crate::ports::text_layout::terminal_cell_width(&self.text)),
        )
        .unwrap_or(u16::MAX)
    }
}

pub(crate) fn action(
    target: HitTarget,
    compact: bool,
    mode: InteractionMode,
    keys: &KeyBindings,
) -> Option<ControlLabel> {
    let editor_mode = matches!(
        mode,
        InteractionMode::Compose | InteractionMode::Edit { .. }
    );
    let (key, text) = match target {
        HitTarget::Insert => (super::settings::key_label(keys.new), " New"),
        HitTarget::Copy => (
            mode_key(editor_mode, compact, ShortcutAction::Copy, keys),
            " Copy",
        ),
        HitTarget::Cut => (
            mode_key(editor_mode, compact, ShortcutAction::Cut, keys),
            " Cut",
        ),
        HitTarget::Delete => (keys.delete_label(), ""),
        HitTarget::Select => (super::settings::key_label(keys.select), " Select"),
        HitTarget::Undo => (
            mode_key(editor_mode, compact, ShortcutAction::Undo, keys),
            " Undo",
        ),
        HitTarget::Search => (super::settings::key_label(keys.search), " Search"),
        HitTarget::Commands => (
            super::settings::key_label(keys.commands),
            if compact { " Menu" } else { " Commands" },
        ),
        HitTarget::Help => (
            super::settings::key_label(keys.help),
            if compact { " Help" } else { " Shortcuts" },
        ),
        HitTarget::Quit => (super::settings::key_label(keys.quit), " Quit"),
        HitTarget::ExitEdit => ("Esc".to_owned(), if compact { "" } else { " Board" }),
        HitTarget::Retry => ("r".to_owned(), " Retry"),
        HitTarget::ExportRecovery => ("w".to_owned(), " Export"),
        HitTarget::BeginDelivery(disposition) | HitTarget::Deliver(_, disposition) => {
            return Some(submission(disposition, mode, keys));
        }
        HitTarget::Agent(_)
        | HitTarget::Thought(_)
        | HitTarget::DragHandle(_)
        | HitTarget::Overflow(_)
        | HitTarget::RenameSession
        | HitTarget::CopySessionId
        | HitTarget::PaletteItem(_)
        | HitTarget::CloseOverlay => return None,
    };
    Some(ControlLabel {
        key,
        text: text.to_owned(),
    })
}

fn mode_key(
    editor_mode: bool,
    compact: bool,
    action: ShortcutAction,
    keys: &KeyBindings,
) -> String {
    if editor_mode {
        shortcut_metadata::canonical_label(action)
    } else {
        shortcut_metadata::board_control_label(action, keys, compact)
    }
}

pub(crate) fn action_width(
    target: HitTarget,
    compact: bool,
    mode: InteractionMode,
    keys: &KeyBindings,
) -> Option<u16> {
    action(target, compact, mode, keys).map(|label| {
        let minimum = match target {
            HitTarget::Insert | HitTarget::Copy | HitTarget::Undo => 7,
            HitTarget::Cut | HitTarget::Delete => 6,
            HitTarget::Search => 9,
            HitTarget::Select => 12,
            HitTarget::Commands => {
                if compact {
                    6
                } else {
                    11
                }
            }
            HitTarget::Help => {
                if compact {
                    6
                } else {
                    12
                }
            }
            HitTarget::ExitEdit => {
                if compact {
                    3
                } else {
                    10
                }
            }
            HitTarget::ExportRecovery => 10,
            HitTarget::Retry => 8,
            _ => 0,
        };
        label.width().max(minimum)
    })
}

pub(crate) fn agent(target: &AgentTarget) -> ControlLabel {
    ControlLabel {
        key: direction_symbol(target.direction).to_owned(),
        text: format!(" {}", compact_agent_name(target.agent_kind.as_str())),
    }
}

pub(crate) fn submission(
    disposition: SubmissionDisposition,
    mode: InteractionMode,
    keys: &KeyBindings,
) -> ControlLabel {
    let editing = matches!(mode, InteractionMode::Edit { .. });
    let (key, text) = match (disposition, editing) {
        (SubmissionDisposition::RemoveAfterSuccess, true) => (
            shortcut_metadata::primary_label(ShortcutAction::Submit),
            " Submit",
        ),
        (SubmissionDisposition::Keep, true) => (
            shortcut_metadata::canonical_label(ShortcutAction::SubmitKeep),
            " Submit & keep",
        ),
        (SubmissionDisposition::RemoveAfterSuccess, false) => {
            (super::settings::key_label(keys.submit_remove), " Submit")
        }
        (SubmissionDisposition::Keep, false) => (
            super::settings::key_label(keys.submit_keep),
            " Submit & keep",
        ),
    };
    ControlLabel {
        key,
        text: text.to_owned(),
    }
}

pub(crate) fn submission_width(
    disposition: SubmissionDisposition,
    mode: InteractionMode,
    keys: &KeyBindings,
) -> u16 {
    let minimum = match disposition {
        SubmissionDisposition::RemoveAfterSuccess => 9,
        SubmissionDisposition::Keep => 16,
    };
    submission(disposition, mode, keys).width().max(minimum)
}

fn compact_agent_name(kind: &str) -> String {
    let mut characters = kind.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

const fn direction_symbol(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "↑",
        Direction::Right => "→",
        Direction::Down => "↓",
        Direction::Left => "←",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_copy_cut_and_undo_labels_share_full_and_compact_measurement() {
        let keys = KeyBindings::default();
        let primary = if cfg!(target_os = "macos") {
            "Command+"
        } else {
            "Ctrl+"
        };
        for (target, suffix, fallback, text) in [
            (HitTarget::Copy, "C", "y", " Copy"),
            (HitTarget::Cut, "X", "x", " Cut"),
            (HitTarget::Undo, "Z", "u", " Undo"),
        ] {
            let full = action(target, false, InteractionMode::Board, &keys).expect("full label");
            assert_eq!(full.key, format!("{primary}{suffix}/{fallback}"));
            assert_eq!(full.text, text);
            assert_eq!(
                action_width(target, false, InteractionMode::Board, &keys),
                Some(full.width())
            );

            let compact =
                action(target, true, InteractionMode::Board, &keys).expect("compact label");
            assert_eq!(compact.key, fallback);
            assert_eq!(compact.text, text);
            assert!(compact.width() <= full.width());
        }
    }
}
