//! Product identity and non-overlapping responsive footer rendering.

use ratatui_core::{
    style::Style,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use crate::{
    application::DurabilityState, domain::Direction, ports::agent::SubmissionDisposition,
    ui::status::StatusSeverity,
};

use super::super::{BoardApp, HitTarget, LayoutSnapshot, Theme};

pub(super) fn render_footer(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    layout: &LayoutSnapshot,
    theme: &Theme,
) {
    render_context(frame, app, layout, theme);
    let keys = app.keybindings();
    for (target, area) in &layout.controls {
        render_control(frame, app, *target, *area, keys, theme);
    }
}

fn render_control(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    target: HitTarget,
    area: ratatui_core::layout::Rect,
    keys: &crate::ui::KeyBindings,
    theme: &Theme,
) {
    let label = if target == HitTarget::RenameSession {
        ControlLabel {
            key: String::new(),
            text: app.session_display_name().to_owned(),
        }
    } else {
        label(app, target, area.width, keys)
    };
    let available = usize::from(area.width).saturating_sub(label.key.width());
    let text = truncate(&label.text, available);
    let line = Line::from(vec![
        Span::styled(label.key, Style::default().fg(theme.accent)),
        Span::styled(text, Style::default().fg(theme.foreground)),
    ]);
    let active_submission = matches!(
        target,
        HitTarget::BeginDelivery(disposition)
            if app.submission_mode() == Some(disposition)
    );
    let interactive = !matches!(target, HitTarget::Agent(_));
    let style = if (interactive && app.hovered() == Some(target)) || active_submission {
        theme.focused_style()
    } else {
        theme.base_style()
    };
    frame.render_widget(Paragraph::new(line).style(style), area);
}

fn render_context(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    if layout.footer_context.height == 0 {
        return;
    }
    let failed = matches!(app.state.durability, DurabilityState::Failed { .. });
    let recovery_only = matches!(
        app.state.durability,
        DurabilityState::Failed {
            code: crate::application::FailureCode::RecoveryCapacity,
            ..
        }
    );
    let status = app.status_view();
    let left = status.map_or_else(
        || {
            if failed {
                if recovery_only {
                    "save failed · w Export recovery"
                } else {
                    "save failed · r Retry · w Export recovery"
                }
            } else {
                ""
            }
        },
        |(message, _)| message,
    );
    let right = &layout.footer_summary;
    let area = inset(layout.footer_context);
    let text = compose(left, right, usize::from(area.width));
    let color = status.map_or_else(
        || if failed { theme.error } else { theme.muted },
        |(_, severity)| match severity {
            StatusSeverity::Info => theme.muted,
            StatusSeverity::Success | StatusSeverity::Warning => theme.accent,
            StatusSeverity::Error => theme.error,
        },
    );
    frame.render_widget(Paragraph::new(text).style(Style::default().fg(color)), area);
}

struct ControlLabel {
    key: String,
    text: String,
}

fn label(
    app: &BoardApp,
    target: HitTarget,
    width: u16,
    keys: &crate::ui::KeyBindings,
) -> ControlLabel {
    let (key, text) = match target {
        HitTarget::Insert => (crate::ui::settings::key_label(keys.new), " New"),
        HitTarget::Copy => (crate::ui::settings::key_label(keys.copy), " Copy"),
        HitTarget::Cut => (crate::ui::settings::key_label(keys.cut), " Cut"),
        HitTarget::Delete => (crate::ui::settings::key_label(keys.delete), " Delete"),
        HitTarget::Undo => (crate::ui::settings::key_label(keys.undo), " Undo"),
        HitTarget::Search => (crate::ui::settings::key_label(keys.search), " Search"),
        HitTarget::Commands => (
            crate::ui::settings::key_label(keys.commands),
            if width <= 6 { " Menu" } else { " Commands" },
        ),
        HitTarget::Help => (
            crate::ui::settings::key_label(keys.help),
            if width <= 6 { " Help" } else { " Shortcuts" },
        ),
        HitTarget::Quit => (crate::ui::settings::key_label(keys.quit), " Quit"),
        HitTarget::ExitEdit => ("Esc".to_owned(), " Board"),
        HitTarget::Retry => ("r".to_owned(), " Retry"),
        HitTarget::ExportRecovery => ("w".to_owned(), " Export"),
        HitTarget::Agent(direction) => {
            let target = app
                .agent_targets()
                .iter()
                .find(|target| target.direction == direction);
            let detail = target.map_or_else(
                || " Agent".to_owned(),
                |target| format!(" {}", compact_agent_name(&target.agent_kind)),
            );
            return ControlLabel {
                key: direction_symbol(direction).to_owned(),
                text: detail,
            };
        }
        HitTarget::BeginDelivery(disposition) | HitTarget::Deliver(_, disposition) => {
            submission_label(disposition, keys)
        }
        _ => (String::new(), ""),
    };
    ControlLabel {
        key,
        text: text.to_owned(),
    }
}

fn submission_label(
    disposition: SubmissionDisposition,
    keys: &crate::ui::KeyBindings,
) -> (String, &'static str) {
    match disposition {
        SubmissionDisposition::RemoveAfterSuccess => (
            crate::ui::settings::key_label(keys.submit_remove),
            " Submit now & remove",
        ),
        SubmissionDisposition::Keep => (
            crate::ui::settings::key_label(keys.submit_keep),
            " Submit now & keep",
        ),
    }
}

fn compact_agent_name(kind: &str) -> String {
    let mut characters = kind.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

fn inset(area: ratatui_core::layout::Rect) -> ratatui_core::layout::Rect {
    ratatui_core::layout::Rect::new(
        area.x.saturating_add(2),
        area.y,
        area.width.saturating_sub(4),
        area.height,
    )
}

fn compose(left: &str, right: &str, width: usize) -> String {
    let right = truncate(right, width);
    let right_width = right.width();
    if right_width >= width {
        return right;
    }
    let left = truncate(left, width.saturating_sub(right_width + 1));
    let left_width = left.width();
    format!(
        "{left}{}{right}",
        " ".repeat(width - left_width - right_width)
    )
}

fn truncate(value: &str, width: usize) -> String {
    let mut cells = 0;
    value
        .graphemes(true)
        .take_while(|grapheme| {
            let next = cells + grapheme.width();
            let keep = next <= width;
            if keep {
                cells = next;
            }
            keep
        })
        .collect()
}

const fn direction_symbol(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "↑",
        Direction::Right => "→",
        Direction::Down => "↓",
        Direction::Left => "←",
    }
}
