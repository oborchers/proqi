//! Product identity and non-overlapping responsive footer rendering.

use ratatui_core::{
    style::Style,
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::paragraph::Paragraph;

use crate::{application::DurabilityState, ui::status::StatusSeverity};

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
    let label = match target {
        HitTarget::Agent(direction) => app
            .agent_targets()
            .iter()
            .find(|target| target.adjacent_direction() == Some(direction))
            .map(crate::ui::control_labels::agent),
        _ => crate::ui::control_labels::action(target, false, app.interaction_mode(), keys)
            .filter(|label| label.width() <= area.width)
            .or_else(|| {
                crate::ui::control_labels::action(target, true, app.interaction_mode(), keys)
            }),
    };
    let Some(label) = label else {
        return;
    };
    let available = usize::from(area.width)
        .saturating_sub(crate::ports::text_layout::terminal_cell_width(&label.key));
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
    let status_area = crate::ui::geometry::inset_horizontal(layout.footer_status, 2);
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
    let state_area = crate::ui::geometry::inset_horizontal(layout.footer_context, 2);
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
    let area = crate::ui::geometry::inset_horizontal(layout.footer_name, 2);
    if area.height == 0 {
        return;
    }
    let name = truncate(&layout.footer_session_name, usize::from(area.width));
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
    let value = truncate(value, usize::from(area.width));
    frame.render_widget(
        Paragraph::new(value).style(theme.focused_style().fg(color)),
        *area,
    );
}

fn truncate(value: &str, width: usize) -> String {
    crate::ports::text_layout::truncate_cells(value, width)
}
