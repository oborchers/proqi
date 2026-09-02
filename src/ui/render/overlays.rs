//! Compact contextual help and searchable modal pickers.

use ratatui_core::{
    style::{Modifier, Style},
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::{
    block::Block,
    borders::Borders,
    clear::Clear,
    paragraph::{Paragraph, Wrap},
};

use super::super::{BoardApp, Theme, layout::OverlayLayout};

pub(super) fn render_help(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    overlay: &OverlayLayout,
    theme: &Theme,
) {
    clear_overlay(frame, overlay.area);
    frame.render_widget(
        Paragraph::new(help_lines(
            app,
            theme,
            overlay.area.width.saturating_sub(2),
            overlay.area.height.saturating_sub(2),
        ))
        .block(
            Block::default()
                .title(Span::styled(
                    " proqi shortcuts ",
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ))
                .style(theme.base_style())
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: true }),
        overlay.area,
    );
    render_overflow_cues(frame, overlay, app.help_overflow(), theme);
    render_close(frame, overlay, theme);
}

pub(super) fn render_picker(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    picker: PickerView<'_>,
    overflow: (bool, bool),
    theme: &Theme,
) {
    clear_overlay(frame, overlay.area);
    frame.render_widget(
        Block::default()
            .title(picker.title)
            .style(theme.base_style())
            .borders(Borders::ALL),
        overlay.area,
    );
    let input = input_area(overlay);
    let available = input.width.saturating_sub(2);
    let query = crate::ports::text_layout::visible_cell_window(
        picker.query,
        picker.cursor,
        usize::from(available),
    );
    frame.render_widget(
        Paragraph::new(format!("{}{}", picker.prompt, query.text)).style(theme.focused_style()),
        input,
    );
    let cursor = u16::try_from(query.cursor_cell).unwrap_or(available);
    frame.set_cursor_position((input.x.saturating_add(1).saturating_add(cursor), input.y));
    for (index, (entry, area)) in picker.entries.iter().zip(&overlay.items).enumerate() {
        if let Some((group, heading)) = entry
            .group
            .zip(overlay.item_headings.get(index).copied().flatten())
        {
            frame.render_widget(
                Paragraph::new(ellipsize(group, usize::from(heading.width))).style(
                    theme
                        .base_style()
                        .fg(theme.muted)
                        .add_modifier(Modifier::BOLD),
                ),
                heading,
            );
        }
        frame.render_widget(
            Paragraph::new(picker_line(
                *entry,
                area.width,
                index == picker.selected,
                theme,
            )),
            *area,
        );
    }
    render_overflow_cues(frame, overlay, overflow, theme);
    render_close(frame, overlay, theme);
}

pub(super) fn render_update(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    title: &str,
    entries: &[String],
    selected: usize,
    theme: &Theme,
) {
    clear_overlay(frame, overlay.area);
    frame.render_widget(
        Block::default()
            .title(Span::styled(
                title,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(theme.base_style())
            .borders(Borders::ALL),
        overlay.area,
    );
    for (index, (entry, area)) in entries.iter().zip(&overlay.items).enumerate() {
        let prefix = if index == selected { "› " } else { "  " };
        let style = if index == selected {
            theme.focused_style().add_modifier(Modifier::BOLD)
        } else {
            theme.base_style()
        };
        frame.render_widget(
            Paragraph::new(format!("{prefix}{entry}")).style(style),
            *area,
        );
    }
    render_close(frame, overlay, theme);
}

pub(super) fn render_text_prompt(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    title: &str,
    value: &str,
    theme: &Theme,
) {
    clear_overlay(frame, overlay.area);
    frame.render_widget(
        Block::default()
            .title(title)
            .style(theme.base_style())
            .borders(Borders::ALL),
        overlay.area,
    );
    let input = input_area(overlay);
    let available = usize::from(input.width.saturating_sub(3));
    let value = crate::ports::text_layout::visible_cell_window(value, value.len(), available);
    frame.render_widget(
        Paragraph::new(format!("> {}", value.text)).style(theme.focused_style()),
        input,
    );
    render_close(frame, overlay, theme);
    let cursor = u16::try_from(value.cursor_cell).unwrap_or(u16::MAX);
    let x = input.x.saturating_add(2).saturating_add(cursor);
    frame.set_cursor_position((x, input.y));
}

pub(super) fn clear_overlay(frame: &mut Frame<'_>, area: ratatui_core::layout::Rect) {
    frame.render_widget(Clear, overlay_clear_area(frame.area(), area));
}

fn overlay_clear_area(
    viewport: ratatui_core::layout::Rect,
    area: ratatui_core::layout::Rect,
) -> ratatui_core::layout::Rect {
    let left = area.x.saturating_sub(1).max(viewport.x);
    let right = area.right().saturating_add(1).min(viewport.right());
    let top = area.y.max(viewport.y);
    let bottom = area.bottom().min(viewport.bottom());
    ratatui_core::layout::Rect::new(
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    )
}

fn input_area(overlay: &OverlayLayout) -> ratatui_core::layout::Rect {
    ratatui_core::layout::Rect::new(
        overlay.area.x.saturating_add(1),
        overlay.area.y.saturating_add(1),
        overlay.area.width.saturating_sub(2),
        u16::from(overlay.area.height > 2),
    )
}

#[derive(Clone, Copy)]
pub(super) struct PickerView<'a> {
    pub(super) title: &'a str,
    pub(super) prompt: char,
    pub(super) query: &'a str,
    pub(super) cursor: usize,
    pub(super) entries: &'a [PickerRow<'a>],
    pub(super) selected: usize,
}

#[derive(Clone, Copy)]
pub(super) struct PickerRow<'a> {
    primary: &'a str,
    secondary: Option<&'a str>,
    secondary_fallbacks: &'a [String],
    group: Option<&'a str>,
}

impl<'a> PickerRow<'a> {
    pub(super) const fn plain(primary: &'a str) -> Self {
        Self {
            primary,
            secondary: None,
            secondary_fallbacks: &[],
            group: None,
        }
    }

    #[cfg(test)]
    pub(super) const fn fields(primary: &'a str, secondary: &'a str) -> Self {
        Self {
            primary,
            secondary: Some(secondary),
            secondary_fallbacks: &[],
            group: None,
        }
    }

    pub(super) const fn grouped(
        primary: &'a str,
        secondary: &'a str,
        secondary_fallbacks: &'a [String],
        group: Option<&'a str>,
    ) -> Self {
        Self {
            primary,
            secondary: Some(secondary),
            secondary_fallbacks,
            group,
        }
    }
}

#[cfg(test)]
fn picker_row(entry: PickerRow<'_>, width: u16) -> String {
    let width = usize::from(width);
    let primary = display(entry.primary);
    let primary_width = cell_width(&primary);
    if let Some(secondary) = fitting_secondary(entry, width) {
        let secondary = display(secondary);
        let gap = width.saturating_sub(primary_width + cell_width(&secondary));
        return format!("{primary}{}{secondary}", " ".repeat(gap));
    }
    ellipsize(entry.primary, width)
}

fn picker_line(entry: PickerRow<'_>, width: u16, selected: bool, theme: &Theme) -> Line<'static> {
    if entry.secondary.is_none() {
        let style = if selected {
            theme
                .focused_style()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.base_style()
        };
        let mut content = ellipsize(entry.primary, usize::from(width));
        content.push_str(&" ".repeat(usize::from(width).saturating_sub(cell_width(&content))));
        return Line::from(Span::styled(content, style));
    }
    let width = usize::from(width);
    let primary = ellipsize(entry.primary, width);
    let base = if selected {
        theme.focused_style()
    } else {
        theme.base_style()
    };
    let primary_style = if selected {
        base.fg(theme.accent).add_modifier(Modifier::BOLD)
    } else {
        base.fg(theme.foreground)
    };
    let Some(secondary) = fitting_secondary(entry, width).map(display) else {
        let padding = " ".repeat(width.saturating_sub(cell_width(&primary)));
        return Line::from(vec![
            Span::styled(primary, primary_style),
            Span::styled(padding, base),
        ]);
    };
    let gap = width.saturating_sub(cell_width(&primary) + cell_width(&secondary));
    Line::from(vec![
        Span::styled(primary, primary_style),
        Span::styled(" ".repeat(gap), base),
        Span::styled(secondary, base.fg(theme.muted)),
    ])
}

fn fitting_secondary(entry: PickerRow<'_>, width: usize) -> Option<&str> {
    let minimum_gap = usize::from(entry.group.is_none()) + 1;
    entry
        .secondary
        .into_iter()
        .chain(entry.secondary_fallbacks.iter().map(String::as_str))
        .find(|secondary| {
            cell_width(entry.primary)
                .saturating_add(minimum_gap)
                .saturating_add(cell_width(secondary))
                <= width
        })
}

pub(super) fn ellipsize(value: &str, width: usize) -> String {
    crate::ports::text_layout::ellipsize_cells(value, width)
}

fn cell_width(value: &str) -> usize {
    crate::ports::text_layout::terminal_cell_width(value)
}

fn display(value: &str) -> String {
    crate::ports::text_layout::truncate_cells(value, cell_width(value))
}

pub(super) fn render_close(frame: &mut Frame<'_>, overlay: &OverlayLayout, theme: &Theme) {
    frame.render_widget(
        Paragraph::new("[x]").style(Style::default().fg(theme.accent)),
        overlay.close,
    );
}

pub(super) fn render_overflow_cues(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    (above, below): (bool, bool),
    theme: &Theme,
) {
    if overlay.area.width == 0 || overlay.area.height < 3 {
        return;
    }
    let x = overlay.area.right().saturating_sub(1);
    if above {
        frame.render_widget(
            Paragraph::new("↑").style(Style::default().fg(theme.muted)),
            ratatui_core::layout::Rect::new(x, overlay.area.y.saturating_add(1), 1, 1),
        );
    }
    if below {
        frame.render_widget(
            Paragraph::new("↓").style(Style::default().fg(theme.muted)),
            ratatui_core::layout::Rect::new(x, overlay.area.bottom().saturating_sub(2), 1, 1),
        );
    }
}

fn help_lines(app: &BoardApp, theme: &Theme, width: u16, height: u16) -> Vec<Line<'static>> {
    let items = crate::ui::shortcuts::items(app);
    let lines = shortcut_grid(&items, width, theme);
    let capacity = usize::from(height);
    let scroll = app.help_scroll().min(lines.len().saturating_sub(capacity));
    lines.into_iter().skip(scroll).take(capacity).collect()
}

fn shortcut_grid(
    items: &[(String, &'static str)],
    width: u16,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let (columns, key_width) = crate::ui::shortcuts::grid_metrics(items, width);
    let cell_width = usize::from(width) / columns;
    items
        .chunks(columns)
        .map(|row| shortcut_row(row, columns, cell_width, key_width, theme))
        .collect()
}

fn shortcut_row(
    items: &[(String, &'static str)],
    columns: usize,
    column_width: usize,
    key_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (key, label)) in items.iter().enumerate() {
        spans.push(Span::styled(key.clone(), Style::default().fg(theme.accent)));
        spans.push(Span::raw(
            " ".repeat(key_width.saturating_sub(cell_width(key)) + 1),
        ));
        spans.push(Span::styled(*label, Style::default().fg(theme.foreground)));
        if index + 1 < columns {
            let used = key_width + 1 + cell_width(label);
            spans.push(Span::raw(" ".repeat(column_width.saturating_sub(used))));
        }
    }
    Line::from(spans)
}

#[cfg(test)]
#[path = "overlays/tests.rs"]
mod tests;
