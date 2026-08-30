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
    let (key, text) = match target {
        HitTarget::Insert => (super::settings::key_label(keys.new), " New"),
        HitTarget::Copy => (super::settings::key_label(keys.copy), " Copy"),
        HitTarget::Cut => (super::settings::key_label(keys.cut), " Cut"),
        HitTarget::Delete => (super::settings::key_label(keys.delete), " Delete"),
        HitTarget::Select => (super::settings::key_label(keys.select), " Select"),
        HitTarget::Undo => (super::settings::key_label(keys.undo), " Undo"),
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
        HitTarget::ExitEdit => ("Esc".to_owned(), " Board"),
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

pub(crate) fn action_width(
    target: HitTarget,
    compact: bool,
    mode: InteractionMode,
    keys: &KeyBindings,
) -> Option<u16> {
    action(target, compact, mode, keys).map(|label| {
        let minimum = match target {
            HitTarget::Insert | HitTarget::Copy | HitTarget::Undo => 7,
            HitTarget::Cut => 6,
            HitTarget::Delete | HitTarget::Search => 9,
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
            HitTarget::ExitEdit | HitTarget::ExportRecovery => 10,
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
        (SubmissionDisposition::RemoveAfterSuccess, true) => {
            (super::settings::primary_key_label("Enter"), " Submit")
        }
        (SubmissionDisposition::Keep, true) => (
            super::settings::primary_key_label("Shift+Enter"),
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
