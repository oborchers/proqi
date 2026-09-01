//! Canonical visible labels and terminal-cell widths for clickable controls.

use crate::{
    application::InteractionMode,
    domain::Direction,
    ports::agent::{AgentTarget, SubmissionDisposition},
};

use super::{HitTarget, KeyBindings};

pub(crate) struct ControlLabel {
    pub(crate) key: String,
    pub(crate) text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubmissionLabelStyle {
    Full,
    Compact,
    KeysOnly,
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
        HitTarget::Copy => (mode_key(editor_mode, "C", keys.copy), " Copy"),
        HitTarget::Cut => (mode_key(editor_mode, "X", keys.cut), " Cut"),
        HitTarget::Delete => (keys.delete_label(), ""),
        HitTarget::Select => (super::settings::key_label(keys.select), " Select"),
        HitTarget::Undo => (mode_key(editor_mode, "Z", keys.undo), " Undo"),
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
            return Some(submission(
                disposition,
                mode,
                keys,
                SubmissionLabelStyle::Full,
            ));
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

fn mode_key(editor_mode: bool, editor_suffix: &str, board_key: char) -> String {
    if editor_mode {
        super::settings::primary_key_label(editor_suffix)
    } else {
        super::settings::key_label(board_key)
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
    style: SubmissionLabelStyle,
) -> ControlLabel {
    let editing = matches!(
        mode,
        InteractionMode::Compose | InteractionMode::Edit { .. }
    );
    let (key, text) = match (disposition, editing, style) {
        (SubmissionDisposition::RemoveAfterSuccess, true, SubmissionLabelStyle::Compact) => {
            (compact_primary_key_label(false), " Send")
        }
        (SubmissionDisposition::Keep, true, SubmissionLabelStyle::Compact) => {
            (compact_primary_key_label(true), " Keep")
        }
        (SubmissionDisposition::RemoveAfterSuccess, false, SubmissionLabelStyle::Compact) => (
            board_compact_submission_key(keys.submit_remove, false),
            " Send",
        ),
        (SubmissionDisposition::Keep, false, SubmissionLabelStyle::Compact) => (
            board_compact_submission_key(keys.submit_keep, true),
            " Keep",
        ),
        (SubmissionDisposition::RemoveAfterSuccess, true, SubmissionLabelStyle::KeysOnly) => {
            (compact_primary_key_label(false), "")
        }
        (SubmissionDisposition::Keep, true, SubmissionLabelStyle::KeysOnly) => {
            (compact_primary_key_label(true), "")
        }
        (SubmissionDisposition::RemoveAfterSuccess, false, SubmissionLabelStyle::KeysOnly) => {
            (super::settings::key_label(keys.submit_remove), "")
        }
        (SubmissionDisposition::Keep, false, SubmissionLabelStyle::KeysOnly) => {
            (super::settings::key_label(keys.submit_keep), "")
        }
        (SubmissionDisposition::RemoveAfterSuccess, true, SubmissionLabelStyle::Full) => {
            (super::settings::primary_key_label("Enter"), " Submit")
        }
        (SubmissionDisposition::Keep, true, SubmissionLabelStyle::Full) => (
            super::settings::primary_key_label("Shift+Enter"),
            " Submit & keep",
        ),
        (SubmissionDisposition::RemoveAfterSuccess, false, SubmissionLabelStyle::Full) => {
            (board_submission_key(keys.submit_remove, "Enter"), " Submit")
        }
        (SubmissionDisposition::Keep, false, SubmissionLabelStyle::Full) => (
            board_submission_key(keys.submit_keep, "Shift+Enter"),
            " Submit & keep",
        ),
    };
    ControlLabel {
        key,
        text: text.to_owned(),
    }
}

pub(crate) fn submission_key(
    disposition: SubmissionDisposition,
    mode: InteractionMode,
    keys: &KeyBindings,
) -> String {
    submission(disposition, mode, keys, SubmissionLabelStyle::Full).key
}

fn board_submission_key(board_key: char, primary_suffix: &str) -> String {
    format!(
        "{}/{}",
        super::settings::key_label(board_key),
        super::settings::primary_key_label(primary_suffix)
    )
}

fn board_compact_submission_key(board_key: char, shifted: bool) -> String {
    format!(
        "{}/{}",
        super::settings::key_label(board_key),
        compact_primary_key_label(shifted)
    )
}

fn compact_primary_key_label(shifted: bool) -> String {
    super::settings::primary_key_label(if shifted { "⇧↵" } else { "↵" })
}

pub(crate) fn submission_width(
    disposition: SubmissionDisposition,
    mode: InteractionMode,
    keys: &KeyBindings,
    style: SubmissionLabelStyle,
) -> u16 {
    submission(disposition, mode, keys, style).width()
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
