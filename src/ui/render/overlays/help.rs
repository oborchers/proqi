//! Contextual shortcut-grid composition.

use ratatui_core::{
    style::Style,
    text::{Line, Span},
};

use crate::ui::{BoardApp, Theme};

pub(super) fn lines(app: &BoardApp, theme: &Theme, width: u16, height: u16) -> Vec<Line<'static>> {
    let items = crate::ui::shortcuts::items(app);
    let lines = grid(&items, width, theme);
    let capacity = usize::from(height);
    let scroll = app.help_scroll().min(lines.len().saturating_sub(capacity));
    lines.into_iter().skip(scroll).take(capacity).collect()
}

fn grid(items: &[crate::ui::shortcuts::Shortcut], width: u16, theme: &Theme) -> Vec<Line<'static>> {
    let (columns, key_width) = crate::ui::shortcuts::grid_metrics(items, width);
    let column_width = usize::from(width) / columns;
    let label_width = items
        .iter()
        .map(|item| cell_width(item.label))
        .max()
        .unwrap_or(1);
    items
        .chunks(columns)
        .map(|row| row_line(row, columns, column_width, key_width, label_width, theme))
        .collect()
}

fn row_line(
    items: &[crate::ui::shortcuts::Shortcut],
    columns: usize,
    column_width: usize,
    key_width: usize,
    label_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let key = crate::ui::shortcuts::key(item, column_width, label_width);
        spans.push(Span::styled(
            key.to_owned(),
            Style::default().fg(theme.accent),
        ));
        spans.push(Span::raw(
            " ".repeat(key_width.saturating_sub(cell_width(key)) + 1),
        ));
        spans.push(Span::styled(
            item.label,
            Style::default().fg(theme.foreground),
        ));
        if index + 1 < columns {
            let used = key_width + 1 + cell_width(item.label);
            spans.push(Span::raw(" ".repeat(column_width.saturating_sub(used))));
        }
    }
    Line::from(spans)
}

fn cell_width(value: &str) -> usize {
    crate::ports::text_layout::terminal_cell_width(value)
}
