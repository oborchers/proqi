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
    let count = app.state.board.live_thoughts().len();
    let left = if layout.header.width >= 40 {
        format!(" proqi · {label} · {count} thoughts")
    } else {
        " proqi".to_owned()
    };
    let text = compose(&left, durability(app), usize::from(layout.header.width));
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
    let text = app.status.as_deref().unwrap_or_else(|| durability(app));
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
    let mode = match app.state.mode {
        InteractionMode::Board => "board",
        InteractionMode::Edit { .. } => "edit",
    };
    let right = format!("{mode} · {}", durability(app));
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
    match app.state.durability {
        DurabilityState::Durable { .. } if app.has_pending_edit() => "saving",
        DurabilityState::Durable { .. } => "saved",
        DurabilityState::Pending { .. } => "saving",
        DurabilityState::Failed { .. } => "save failed",
    }
}

fn label(target: HitTarget, width: u16, keys: &crate::ui::KeyBindings) -> String {
    let full = match target {
        HitTarget::Insert => format!("{} New", keys.new),
        HitTarget::Copy => format!("{} Copy", keys.copy),
        HitTarget::Cut => format!("{} Cut", keys.cut),
        HitTarget::Delete => format!("{} Delete", keys.delete),
        HitTarget::Undo => format!("{} Undo", keys.undo),
        HitTarget::Search => format!("{} Search", keys.search),
        HitTarget::Commands => format!("{} Commands", keys.commands),
        HitTarget::Help => format!("{} Shortcuts", keys.help),
        HitTarget::Quit => format!("{} Quit", keys.quit),
        HitTarget::Submit(direction, remove) => {
            let key = if remove {
                keys.submit_remove
            } else {
                keys.submit
            };
            let verb = if remove { "Send+" } else { "Send" };
            format!("{key}{} {verb}", direction_symbol(direction))
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
