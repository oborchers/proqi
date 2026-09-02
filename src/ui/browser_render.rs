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
    for (cue, symbol) in [(layout.overflow_above, "↑"), (layout.overflow_below, "↓")] {
        if let Some(area) = cue {
            frame.render_widget(
                Paragraph::new(symbol).style(Style::default().fg(theme.muted)),
                area,
            );
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
    let title = crate::ui::geometry::row(layout.header, 0);
    frame.render_widget(
        Paragraph::new(" Resume a Proqi session").style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        title,
    );
    if layout.header.height > 1 {
        let rename = browser.rename_value();
        let (label, value) =
            rename.map_or((" Search: ", browser.query()), |value| (" Rename: ", value));
        let available = usize::from(layout.header.width)
            .saturating_sub(crate::ports::text_layout::terminal_cell_width(label))
            .saturating_sub(1);
        let value = crate::ports::text_layout::visible_cell_window(value, value.len(), available);
        let style = if rename.is_some() {
            theme.focused_style()
        } else {
            theme.base_style()
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(label, Style::default().fg(theme.muted)),
                Span::raw(value.text),
                Span::styled("_", Style::default().fg(theme.accent)),
            ]))
            .style(style),
            crate::ui::geometry::row(layout.header, 1),
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
    let fixed_cells =
        4_usize.saturating_add(crate::ports::text_layout::terminal_cell_width(&badge));
    let label_cells = usize::from(area.width).saturating_sub(fixed_cells);
    let style = if selected {
        theme.focused_style().fg(theme.accent)
    } else {
        theme.base_style()
    }
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
        crate::ui::geometry::row(area, 0),
    );
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new(format!(
                "  {} thoughts  {activity}  {}",
                item.hit.thought_count,
                item.hit.last_opened_cwd.display()
            ))
            .style(Style::default().fg(theme.muted)),
            crate::ui::geometry::row(area, 1),
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
            let direction = context.direction.as_str();
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
    if let Some(status) = browser.status.as_deref() {
        frame.render_widget(
            Paragraph::new(status).style(Style::default().fg(theme.error)),
            layout.footer,
        );
        return;
    }
    for control in super::browser::browser_footer_controls(layout.footer) {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(control.key, Style::default().fg(theme.accent)),
                Span::styled(
                    format!(" {}", control.label),
                    Style::default().fg(theme.foreground),
                ),
            ])),
            control.area,
        );
    }
}
