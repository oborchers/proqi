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
    let keys = app.shortcut_registry();
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
    keys: &crate::ui::ShortcutRegistry,
    theme: &Theme,
) {
    let context = app.footer_shortcut_context();
    let label = match target {
        HitTarget::Agent(direction) => app
            .agent_targets()
            .iter()
            .find(|target| target.adjacent_direction() == Some(direction))
            .map(crate::ui::control_labels::agent),
        HitTarget::CommitThoughtName | HitTarget::CancelThoughtName => {
            crate::ui::control_labels::thought_name_action(target, area.width)
        }
        _ => crate::ui::control_labels::action(target, false, context, keys)
            .filter(|label| label.width() <= area.width)
            .or_else(|| crate::ui::control_labels::action(target, true, context, keys)),
    };
    let Some(label) = label else {
        return;
    };
    let available = usize::from(area.width)
        .saturating_sub(crate::ports::text_layout::terminal_cell_width(&label.key));
    let text = truncate(&label.text, available);
    let active_submission = matches!(
        target,
        HitTarget::BeginDelivery(disposition)
            if app.submission_mode() == Some(disposition)
    );
    let interactive = !matches!(target, HitTarget::Agent(_));
    let hovered = interactive && app.hovered() == Some(target);
    let style = if hovered {
        theme.control_hovered_style()
    } else if active_submission {
        theme.focused_style()
    } else {
        theme.base_style()
    };
    let line = Line::from(vec![
        Span::styled(label.key, style.fg(theme.accent)),
        Span::styled(text, style.fg(theme.foreground)),
    ]);
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
    let recovery = if failed {
        use crate::ui::{ShortcutActionId as Action, ShortcutContext as Context};
        let registry = app.shortcut_registry();
        let export = registry.action_label(Context::Recovery, Action::ExportRecovery, true);
        if recovery_only {
            format!("save failed · {export} Export recovery")
        } else {
            let retry = registry.action_label(Context::Recovery, Action::RetryStorage, true);
            format!("save failed · {retry} Retry · {export} Export recovery")
        }
    } else {
        String::new()
    };
    let screenshot = (!layout.optional_footer_chrome_visible)
        .then(|| app.screenshot_footer_state(false))
        .flatten();
    let primary = status.map(|(message, _)| message);
    let status_area = crate::ui::geometry::inset_horizontal(layout.footer_status, 2);
    let left = if layout.optional_footer_chrome_visible {
        primary
            .or_else(|| (!recovery.is_empty()).then_some(recovery.as_str()))
            .unwrap_or_default()
            .to_owned()
    } else {
        hidden_status_text(
            app,
            failed,
            &recovery,
            screenshot.as_deref(),
            status,
            usize::from(status_area.width),
        )
    };
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
            Paragraph::new(truncate(&left, usize::from(status_area.width)))
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

fn hidden_status_text(
    app: &BoardApp,
    failed: bool,
    recovery: &str,
    screenshot: Option<&str>,
    status: Option<(&str, StatusSeverity)>,
    available: usize,
) -> String {
    if failed {
        return hidden_failure_status(app, recovery, available);
    }

    let durability = app.durability_footer_status();
    let primary = status.map(|(message, _)| message);
    let detailed = join_footer_states([screenshot, durability, primary]);
    let compact_screenshot = app.screenshot_footer_state(true);
    let micro_screenshot = compact_screenshot.as_ref().map(|label| {
        if label.contains("paused") {
            "paused"
        } else {
            "inbox"
        }
    });
    let compact = join_footer_states([compact_screenshot.as_deref(), durability, primary]);
    let safety = join_footer_states([compact_screenshot.as_deref(), durability]);
    let terse_safety = compact_screenshot.as_ref().and_then(|label| {
        durability.map(|_| {
            if label.contains("paused") {
                "saving paused"
            } else {
                "saving · inbox"
            }
        })
    });
    let micro_safety = compact_screenshot.as_ref().and_then(|label| {
        durability.map(|_| {
            if label.contains("paused") {
                "pause/s"
            } else {
                "inbox/s"
            }
        })
    });
    let terse_status = compact_screenshot.as_ref().and_then(|label| {
        status.and_then(|(_, severity)| match severity {
            StatusSeverity::Warning if label.contains("paused") => Some("warn · paused"),
            StatusSeverity::Warning => Some("warn · inbox"),
            StatusSeverity::Error if label.contains("paused") => Some("error · paused"),
            StatusSeverity::Error => Some("error · inbox"),
            StatusSeverity::Info | StatusSeverity::Success => None,
        })
    });
    let micro_status = compact_screenshot.as_ref().and_then(|label| {
        status.and_then(|(_, severity)| match severity {
            StatusSeverity::Warning if label.contains("paused") => Some("warn/p"),
            StatusSeverity::Warning => Some("warn/i"),
            StatusSeverity::Error if label.contains("paused") => Some("error/p"),
            StatusSeverity::Error => Some("error/i"),
            StatusSeverity::Info | StatusSeverity::Success => None,
        })
    });
    let compact_status = status.and_then(|(_, severity)| match severity {
        StatusSeverity::Warning => Some("warn"),
        StatusSeverity::Error => Some("error"),
        StatusSeverity::Info | StatusSeverity::Success => None,
    });
    first_fitting_footer_state(
        [
            Some(detailed.as_str()),
            Some(compact.as_str()),
            terse_safety,
            terse_status,
            micro_safety,
            micro_status,
            Some(safety.as_str()),
            micro_screenshot,
            compact_screenshot.as_deref(),
            durability,
            compact_status,
            primary,
        ],
        available,
    )
}

fn hidden_failure_status(app: &BoardApp, recovery: &str, available: usize) -> String {
    let compact_screenshot = app.screenshot_footer_state(true);
    let detailed = join_footer_states([Some(recovery), compact_screenshot.as_deref()]);
    let concise = join_footer_states([Some("save failed"), compact_screenshot.as_deref()]);
    let terse = compact_screenshot.as_ref().map(|label| {
        if label.contains("paused") {
            "failed · paused"
        } else {
            "failed · inbox"
        }
    });
    let micro = compact_screenshot.as_ref().map(|label| {
        if label.contains("paused") {
            "fail/p"
        } else {
            "fail/i"
        }
    });
    first_fitting_footer_state(
        [
            Some(detailed.as_str()),
            Some(concise.as_str()),
            terse,
            micro,
            Some("failed"),
        ],
        available,
    )
}

fn join_footer_states<const N: usize>(states: [Option<&str>; N]) -> String {
    states.into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

fn first_fitting_footer_state<const N: usize>(
    candidates: [Option<&str>; N],
    available: usize,
) -> String {
    let mut fallback = None;
    for candidate in candidates
        .into_iter()
        .flatten()
        .filter(|candidate| !candidate.is_empty())
    {
        if crate::ports::text_layout::terminal_cell_width(candidate) <= available {
            return candidate.to_owned();
        }
        fallback = Some(candidate);
    }
    fallback.unwrap_or_default().to_owned()
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
        Paragraph::new(value).style(theme.control_hovered_style().fg(color)),
        *area,
    );
}

fn truncate(value: &str, width: usize) -> String {
    crate::ports::text_layout::truncate_cells(value, width)
}
