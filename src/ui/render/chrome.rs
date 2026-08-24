//! Product identity and non-overlapping responsive footer rendering.

use ratatui_core::{
    style::{Modifier, Style},
    terminal::Frame,
};
use ratatui_widgets::paragraph::Paragraph;
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use crate::{
    application::{DurabilityState, InteractionMode},
    domain::Direction,
};

use super::super::{BoardApp, HitTarget, LayoutSnapshot, Theme};

pub(super) fn render_header(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    layout: &LayoutSnapshot,
    theme: &Theme,
) {
    if layout.header.height == 0 {
        return;
    }
    let session = &app.state.board.session;
    let label = session.name.clone().unwrap_or_else(|| {
        session.last_opened_cwd.file_name().map_or_else(
            || "untitled".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    });
    let count = app.visible_thought_count();
    let noun = if count == 1 { "thought" } else { "thoughts" };
    let left = if layout.header.width >= 40 {
        format!(" proqi · {label} · {count} {noun}")
    } else {
        " proqi".to_owned()
    };
    let text = compose(
        &left,
        summary_durability(app),
        usize::from(layout.header.width),
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(theme.muted)),
        layout.header,
    );
}

pub(super) fn render_footer(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    layout: &LayoutSnapshot,
    theme: &Theme,
) {
    render_status(frame, app, layout, theme);
    render_context(frame, app, layout, theme);
    let keys = app.keybindings();
    for (target, area) in &layout.controls {
        let label = label(*target, area.width, keys);
        let style = if app.hovered() == Some(*target) {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme.accent)
        };
        frame.render_widget(Paragraph::new(label).style(style), *area);
    }
}

fn render_status(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    let Some(area) = layout.footer_status else {
        return;
    };
    let text = app.status.as_deref().unwrap_or_else(|| {
        if matches!(app.state.durability, DurabilityState::Failed { .. }) {
            "save failed · r Retry · w Export recovery"
        } else {
            durability(app)
        }
    });
    let color = if matches!(app.state.durability, DurabilityState::Failed { .. }) {
        theme.error
    } else {
        theme.muted
    };
    frame.render_widget(
        Paragraph::new(format!(" {text}")).style(Style::default().fg(color)),
        area,
    );
}

fn render_context(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    if layout.footer_context.height == 0 {
        return;
    }
    let fallback = app.agent_hint().unwrap_or_else(|| "ready".to_owned());
    let left = layout
        .footer_status
        .is_none()
        .then(|| app.status.clone())
        .flatten()
        .unwrap_or(fallback);
    let mode = match app.interaction_mode() {
        InteractionMode::Board => "board",
        InteractionMode::Edit { .. } => "edit",
    };
    let right = format!("{mode} · {}", summary_durability(app));
    let text = compose(
        &format!(" {left}"),
        &right,
        usize::from(layout.footer_context.width),
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(theme.muted)),
        layout.footer_context,
    );
}

fn durability(app: &BoardApp) -> &'static str {
    if app.has_draft() {
        return "draft";
    }
    match app.state.durability {
        DurabilityState::Durable { .. } if app.has_pending_edit() => "saving",
        DurabilityState::Durable { .. } => "saved",
        DurabilityState::Pending { .. } => "saving",
        DurabilityState::Failed { .. } => "save failed",
    }
}

fn summary_durability(app: &BoardApp) -> &'static str {
    if matches!(app.state.durability, DurabilityState::Failed { .. }) {
        "unsaved"
    } else {
        durability(app)
    }
}

fn label(target: HitTarget, width: u16, keys: &crate::ui::KeyBindings) -> String {
    let full = match target {
        HitTarget::Insert => format!("{} New", crate::ui::settings::key_label(keys.new)),
        HitTarget::Copy => format!("{} Copy", crate::ui::settings::key_label(keys.copy)),
        HitTarget::Cut => format!("{} Cut", crate::ui::settings::key_label(keys.cut)),
        HitTarget::Delete => format!("{} Delete", crate::ui::settings::key_label(keys.delete)),
        HitTarget::Undo => format!("{} Undo", crate::ui::settings::key_label(keys.undo)),
        HitTarget::Search => format!("{} Search", crate::ui::settings::key_label(keys.search)),
        HitTarget::Commands => {
            format!("{} Commands", crate::ui::settings::key_label(keys.commands))
        }
        HitTarget::Help => format!("{} Shortcuts", crate::ui::settings::key_label(keys.help)),
        HitTarget::Quit => format!("{} Quit", crate::ui::settings::key_label(keys.quit)),
        HitTarget::ExitEdit => "Esc Board".to_owned(),
        HitTarget::Retry => "r Retry".to_owned(),
        HitTarget::ExportRecovery => "w Export".to_owned(),
        HitTarget::Submit(direction, remove) => {
            let key = if remove {
                keys.submit_remove
            } else {
                keys.submit
            };
            let verb = if remove { "Send+" } else { "Send" };
            format!(
                "{}{} {verb}",
                crate::ui::settings::key_label(key),
                direction_symbol(direction)
            )
        }
        _ => String::new(),
    };
    if width <= 6 {
        return match target {
            HitTarget::Commands => format!("{} Menu", keys.commands),
            HitTarget::Help => format!("{} Help", keys.help),
            _ => truncate(&full, usize::from(width)),
        };
    }
    truncate(&full, usize::from(width))
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
