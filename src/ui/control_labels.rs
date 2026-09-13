//! Canonical visible labels and terminal-cell widths for clickable controls.

use crate::{
    application::InteractionMode,
    domain::Direction,
    ports::agent::{AgentTarget, SubmissionDisposition},
};

use super::shortcut_registry::presentation;
use super::{HitTarget, ShortcutActionId as ShortcutAction, ShortcutRegistry};

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
    context: super::ShortcutContext,
    keys: &ShortcutRegistry,
) -> Option<ControlLabel> {
    let action = target_action(target)?;
    let projection = presentation::footer_projection(action, compact, context, keys)?;
    Some(ControlLabel {
        key: projection.key,
        text: projection.text.to_owned(),
    })
}

pub(crate) fn action_width(
    target: HitTarget,
    compact: bool,
    context: super::ShortcutContext,
    keys: &ShortcutRegistry,
) -> Option<u16> {
    let action_id = target_action(target)?;
    let projection = presentation::footer_projection(action_id, compact, context, keys)?;
    action(target, compact, context, keys).map(|label| label.width().max(projection.minimum_width))
}

const fn target_action(target: HitTarget) -> Option<ShortcutAction> {
    match target {
        HitTarget::Insert => Some(ShortcutAction::New),
        HitTarget::Copy => Some(ShortcutAction::Copy),
        HitTarget::Cut => Some(ShortcutAction::Cut),
        HitTarget::Delete => Some(ShortcutAction::Delete),
        HitTarget::Select => Some(ShortcutAction::Select),
        HitTarget::Undo => Some(ShortcutAction::Undo),
        HitTarget::Redo => Some(ShortcutAction::Redo),
        HitTarget::Search => Some(ShortcutAction::OpenSearch),
        HitTarget::Commands => Some(ShortcutAction::OpenCommands),
        HitTarget::Help => Some(ShortcutAction::Help),
        HitTarget::Quit => Some(ShortcutAction::Quit),
        HitTarget::ExitEdit => Some(ShortcutAction::Close),
        HitTarget::Retry => Some(ShortcutAction::RetryStorage),
        HitTarget::ExportRecovery => Some(ShortcutAction::ExportRecovery),
        HitTarget::BeginDelivery(disposition) | HitTarget::Deliver(_, disposition) => {
            Some(submission_action(disposition))
        }
        _ => None,
    }
}

pub(crate) fn agent(target: &AgentTarget) -> ControlLabel {
    ControlLabel {
        key: target
            .adjacent_direction()
            .map(direction_symbol)
            .unwrap_or_default()
            .to_owned(),
        text: format!(" {}", compact_agent_name(target.agent_kind().as_str())),
    }
}

pub(crate) fn submission(
    disposition: SubmissionDisposition,
    context: super::ShortcutContext,
    keys: &ShortcutRegistry,
) -> Option<ControlLabel> {
    let action = submission_action(disposition);
    let projection = presentation::footer_projection(action, false, context, keys)?;
    Some(ControlLabel {
        key: projection.key,
        text: projection.text.to_owned(),
    })
}

pub(crate) fn submission_width(
    disposition: SubmissionDisposition,
    context: super::ShortcutContext,
    keys: &ShortcutRegistry,
) -> Option<u16> {
    let action = submission_action(disposition);
    let projection = presentation::footer_projection(action, false, context, keys)?;
    submission(disposition, context, keys).map(|label| label.width().max(projection.minimum_width))
}

const fn submission_action(disposition: SubmissionDisposition) -> ShortcutAction {
    match disposition {
        SubmissionDisposition::RemoveAfterSuccess => ShortcutAction::SubmitRemove,
        SubmissionDisposition::Keep => ShortcutAction::SubmitKeep,
    }
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
        let keys = ShortcutRegistry::default();
        let primary = if cfg!(target_os = "macos") {
            "Cmd+"
        } else {
            "Ctrl+"
        };
        for (target, full_key, compact_key, text) in [
            (HitTarget::Copy, format!("{primary}C/y"), "y", " Copy"),
            (HitTarget::Cut, format!("{primary}X/x"), "x", " Cut"),
            (
                HitTarget::Undo,
                if cfg!(target_os = "macos") {
                    "Ctrl+Z".to_owned()
                } else {
                    format!("{primary}Z/u")
                },
                if cfg!(target_os = "macos") {
                    "Ctrl+Z"
                } else {
                    "u"
                },
                " Undo",
            ),
        ] {
            let full = action(target, false, super::super::ShortcutContext::Board, &keys)
                .expect("full label");
            assert_eq!(full.key, full_key);
            assert_eq!(full.text, text);
            assert_eq!(
                action_width(target, false, super::super::ShortcutContext::Board, &keys),
                Some(full.width())
            );

            let compact = action(target, true, super::super::ShortcutContext::Board, &keys)
                .expect("compact label");
            assert_eq!(compact.key, compact_key);
            assert_eq!(compact.text, text);
            assert!(compact.width() <= full.width());
        }
    }
}
