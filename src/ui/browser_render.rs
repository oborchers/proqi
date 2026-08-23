//! Complete Ratatui renderer for the responsive session browser.

use ratatui_core::{
    style::{Modifier, Style},
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::{block::Block, borders::Borders, paragraph::Paragraph};

use super::{BrowserAvailability, BrowserLayout, SessionBrowser, Theme, browser_summary::summary};

/// Render the complete session browser frame.
pub fn render_browser(
    frame: &mut Frame<'_>,
    browser: &SessionBrowser,
    layout: &BrowserLayout,
    theme: &Theme,
) {
    frame.render_widget(Block::default().style(theme.base_style()), layout.area);
    render_header(frame, browser, layout, theme);
    for entry in &layout.entries {
        let Some(item) = browser
            .visible_items()
            .find_map(|(index, item)| (index == entry.item_index).then_some(item))
        else {
            continue;
        };
        if let Some((group, area)) = entry.group {
            frame.render_widget(
                Paragraph::new(format!(" {}", group.label())).style(
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::BOLD),
                ),
                area,
            );
        }
        let selected = browser
            .selected_item()
            .is_some_and(|(index, _)| index == entry.item_index);
        render_result(
            frame,
            item,
            entry.row,
            selected,
            &browser.activity_label(item),
            theme,
        );
        if let Some(area) = entry.inline_detail {
            render_detail(frame, item, area, theme);
        }
    }
    if browser.visible_items().next().is_none() {
        frame.render_widget(
            Paragraph::new(" No matching sessions").style(Style::default().fg(theme.muted)),
            layout.results,
        );
    }
    if let Some(area) = layout.detail
        && let Some((_, item)) = browser.selected_item()
    {
        render_detail(frame, item, area, theme);
    }
    render_footer(frame, browser, layout, theme);
}

fn render_header(
    frame: &mut Frame<'_>,
    browser: &SessionBrowser,
    layout: &BrowserLayout,
    theme: &Theme,
) {
    let title = RectLine::new(layout.header, 0);
    frame.render_widget(
        Paragraph::new(" Resume a Proqi session").style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        title.area,
    );
    if layout.header.height > 1 {
        let (label, value) = browser
            .rename_value()
            .map_or((" Search: ", browser.query()), |value| (" Rename: ", value));
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(label, Style::default().fg(theme.muted)),
                Span::raw(value.to_owned()),
                Span::styled("_", Style::default().fg(theme.accent)),
            ])),
            RectLine::new(layout.header, 1).area,
        );
    }
}

fn render_result(
    frame: &mut Frame<'_>,
    item: &super::SessionBrowserItem,
    area: ratatui_core::layout::Rect,
    selected: bool,
    activity: &str,
    theme: &Theme,
) {
    let label = item
        .hit
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            if item.hit.excerpt.is_empty() {
                "Untitled session"
            } else {
                &item.hit.excerpt
            }
        });
    let focus = if selected { "│" } else { " " };
    let badge = format!("[{}]", item.availability.label());
    let fixed_cells = 4_usize.saturating_add(unicode_width::UnicodeWidthStr::width(badge.as_str()));
    let label_cells = usize::from(area.width).saturating_sub(fixed_cells);
    let style = Style::default()
        .fg(if selected {
            theme.accent
        } else {
            theme.foreground
        })
        .add_modifier(if selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
    frame.render_widget(
        Paragraph::new(format!(
            "{focus} {}  [{}]",
            summary(label, label_cells),
            item.availability.label()
        ))
        .style(style),
        RectLine::new(area, 0).area,
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(format!(
                "  {} thoughts  {activity}  {}",
                item.hit.thought_count,
                item.hit.last_opened_cwd.display()
            ))
            .style(Style::default().fg(theme.muted)),
            RectLine::new(area, 1).area,
        );
    }
}

fn render_detail(
    frame: &mut Frame<'_>,
    item: &super::SessionBrowserItem,
    area: ratatui_core::layout::Rect,
    theme: &Theme,
) {
    let border = Block::default()
        .borders(Borders::LEFT)
        .style(Style::default().fg(theme.muted));
    let inner = border.inner(area);
    frame.render_widget(border, area);
    let (integration, agent) = integration_lines(item);
    let owner = match &item.availability {
        BrowserAvailability::Active(instance) => format!(
            "owner: pid {} from {}",
            instance.pid, instance.launch_directory
        ),
        _ => format!("state: {}", item.availability.label()),
    };
    let preview = item
        .hit
        .previews
        .iter()
        .map(|value| summary(value, usize::from(inner.width.saturating_sub(3))))
        .collect::<Vec<_>>()
        .join(" | ");
    let lines = [
        format!(" {}", item.hit.id),
        format!(" {owner}"),
        format!(" origin: {}", item.hit.origin_cwd.display()),
        format!(" opened from: {}", item.hit.last_opened_cwd.display()),
        format!(
            " opened: {}  active: {}",
            item.hit.last_opened_at.as_millis(),
            item.hit.last_active_at.as_millis()
        ),
        format!(" {integration}"),
        format!(" {agent}"),
        format!(" {preview}"),
    ];
    frame.render_widget(
        Paragraph::new(lines.join("\n")).style(Style::default().fg(theme.muted)),
        inner,
    );
}

fn integration_lines(item: &super::SessionBrowserItem) -> (String, String) {
    item.hit.integration_context.as_ref().map_or_else(
        || ("integration: none".to_owned(), "agent: none".to_owned()),
        |context| {
            let direction = match context.direction {
                crate::domain::Direction::Up => "up",
                crate::domain::Direction::Right => "right",
                crate::domain::Direction::Down => "down",
                crate::domain::Direction::Left => "left",
            };
            (
                format!("integration: {} / {direction}", context.provider),
                format!("agent: {} ({})", context.agent_name, context.agent_kind),
            )
        },
    )
}

fn render_footer(
    frame: &mut Frame<'_>,
    browser: &SessionBrowser,
    layout: &BrowserLayout,
    theme: &Theme,
) {
    let text = browser
        .status
        .as_deref()
        .unwrap_or(" [R] rename  [D] trash  ↑↓ select  enter open  esc cancel");
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(if browser.status.is_some() {
            theme.error
        } else {
            theme.muted
        })),
        layout.footer,
    );
}

struct RectLine {
    area: ratatui_core::layout::Rect,
}

impl RectLine {
    fn new(area: ratatui_core::layout::Rect, offset: u16) -> Self {
        Self {
            area: ratatui_core::layout::Rect::new(
                area.x,
                area.y.saturating_add(offset),
                area.width,
                u16::from(offset < area.height),
            ),
        }
    }
}
