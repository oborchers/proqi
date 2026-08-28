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
    render_session_identity(frame, app, layout, theme);
    let keys = app.keybindings();
    for (target, area) in &layout.controls {
        if matches!(target, HitTarget::RenameSession | HitTarget::CopySessionId) {
            continue;
        }
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
    let status_area = inset(layout.footer_status);
    let color = status.map_or_else(
        || if failed { theme.error } else { theme.muted },
        |(_, severity)| match severity {
            StatusSeverity::Info => theme.muted,
            StatusSeverity::Success => theme.success,
            StatusSeverity::Warning => theme.warning,
            StatusSeverity::Error => theme.error,
        },
    );
    if status_area.height > 0 && !left.is_empty() {
        frame.render_widget(
            Paragraph::new(truncate(left, usize::from(status_area.width)))
                .style(Style::default().fg(color)),
            status_area,
        );
    }
    let state_area = inset(layout.footer_context);
    if state_area.height > 0 {
        frame.render_widget(
            Paragraph::new(truncate(
                &layout.footer_summary,
                usize::from(state_area.width),
            ))
            .style(Style::default().fg(theme.muted)),
            state_area,
        );
    }
}

fn render_session_identity(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    layout: &LayoutSnapshot,
    theme: &Theme,
) {
    let area = inset(layout.footer_name);
    if area.height == 0 {
        return;
    }
    let name = if layout.footer_session_id.is_some() {
        layout.footer_session_name.clone()
    } else {
        truncate(&layout.footer_session_name, usize::from(area.width))
    };
    let mut spans = vec![Span::styled(name, Style::default().fg(theme.foreground))];
    if let Some(session_id) = &layout.footer_session_id {
        spans.push(Span::styled(" · ", Style::default().fg(theme.muted)));
        spans.push(Span::styled(
            session_id.clone(),
            Style::default().fg(theme.muted),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.base_style()),
        area,
    );
    render_identity_hover(frame, app, layout, theme, HitTarget::RenameSession);
    render_identity_hover(frame, app, layout, theme, HitTarget::CopySessionId);
}

fn render_identity_hover(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    layout: &LayoutSnapshot,
    theme: &Theme,
    target: HitTarget,
) {
    if app.hovered() != Some(target) {
        return;
    }
    let Some((_, area)) = layout
        .controls
        .iter()
        .find(|(candidate, _)| *candidate == target)
    else {
        return;
    };
    let (value, color) = match target {
        HitTarget::RenameSession => (&layout.footer_session_name, theme.foreground),
        HitTarget::CopySessionId => {
            let Some(session_id) = &layout.footer_session_id else {
                return;
            };
            (session_id, theme.muted)
        }
        _ => return,
    };
    frame.render_widget(
        Paragraph::new(value.as_str()).style(theme.focused_style().fg(color)),
        *area,
    );
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
        HitTarget::Select => (crate::ui::settings::key_label(keys.select), " Select"),
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
                |target| format!(" {}", compact_agent_name(target.agent_kind.as_str())),
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
            " Submit",
        ),
        SubmissionDisposition::Keep => (
            crate::ui::settings::key_label(keys.submit_keep),
            " Submit & keep",
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
