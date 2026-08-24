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
    let identity = if label.eq_ignore_ascii_case("proqi") {
        "proqi".to_owned()
    } else {
        format!("proqi · {label}")
    };
    let left = if layout.header.width >= 40 {
        format!("  {identity} · {count} {noun}")
    } else if layout.header.width < 20 {
        " proqi".to_owned()
    } else {
        "  proqi".to_owned()
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
    let label = label(app, target, area.width, keys);
    let available = usize::from(area.width).saturating_sub(label.key.width());
    let text = truncate(&label.text, available);
    let line = Line::from(vec![
        Span::styled(label.key, Style::default().fg(theme.accent)),
        Span::styled(text, Style::default().fg(theme.foreground)),
    ]);
    let active_submission =
        matches!(target, HitTarget::BeginSubmit(remove) if app.submission_mode() == Some(remove));
    let style = if app.hovered() == Some(target) || active_submission {
        theme.focused_style()
    } else {
        theme.base_style()
    };
    frame.render_widget(Paragraph::new(line).style(style), area);
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
        Paragraph::new(text).style(Style::default().fg(color)),
        inset(area),
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
    let area = inset(layout.footer_context);
    let text = compose(&left, &right, usize::from(area.width));
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(theme.muted)),
        area,
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
        HitTarget::BeginSubmit(remove) => submit_mode_label(remove, keys),
        HitTarget::Submit(direction, remove) => {
            let target = app
                .agent_targets()
                .iter()
                .find(|target| target.direction == direction);
            if app.agent_targets().len() == 1 {
                let key = if remove {
                    keys.submit_remove
                } else {
                    keys.submit
                };
                let verb = if remove { "Send+" } else { "Send" };
                let detail = target.map_or_else(
                    || format!(" {verb} {}", direction_word(direction)),
                    |target| {
                        format!(
                            " {verb} {} · {} · {}",
                            direction_word(direction),
                            compact_agent_name(&target.agent_kind),
                            readiness(target.readiness)
                        )
                    },
                );
                return ControlLabel {
                    key: crate::ui::settings::key_label(key),
                    text: detail,
                };
            }
            let detail = target.map_or_else(
                || " Agent".to_owned(),
                |target| {
                    format!(
                        " {} · {}",
                        compact_agent_name(&target.agent_kind),
                        readiness(target.readiness)
                    )
                },
            );
            return ControlLabel {
                key: direction_symbol(direction).to_owned(),
                text: detail,
            };
        }
        _ => (String::new(), ""),
    };
    ControlLabel {
        key,
        text: text.to_owned(),
    }
}

fn submit_mode_label(remove: bool, keys: &crate::ui::KeyBindings) -> (String, &'static str) {
    if remove {
        (crate::ui::settings::key_label(keys.submit_remove), " Send+")
    } else {
        (crate::ui::settings::key_label(keys.submit), " Send")
    }
}

fn compact_agent_name(kind: &str) -> String {
    let mut characters = kind.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}

const fn readiness(value: crate::ports::agent::AgentReadiness) -> &'static str {
    match value {
        crate::ports::agent::AgentReadiness::Idle => "idle",
        crate::ports::agent::AgentReadiness::Working => "working",
        crate::ports::agent::AgentReadiness::Done => "done",
    }
}

const fn direction_word(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Right => "right",
        Direction::Down => "down",
        Direction::Left => "left",
    }
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
