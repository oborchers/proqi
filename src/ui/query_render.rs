//! Shared single-line query rendering with directional selection.

use ratatui_core::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_segmentation::UnicodeSegmentation as _;

use super::{Theme, app::query::QuerySelection};

pub(in crate::ui) fn input_line(
    prefix: impl Into<String>,
    value: &str,
    window: &crate::ports::text_layout::VisibleCellWindow,
    selection: Option<QuerySelection>,
    theme: &Theme,
) -> Line<'static> {
    let selected = selection.map(|selection| {
        let anchor = crate::ports::text_layout::cursor_cell(value, selection.anchor);
        let head = crate::ports::text_layout::cursor_cell(value, selection.head);
        anchor.min(head).saturating_sub(window.start_cell)
            ..anchor.max(head).saturating_sub(window.start_cell)
    });
    let mut spans = vec![Span::raw(prefix.into())];
    let mut column = 0_usize;
    spans.extend(window.text.graphemes(true).map(|grapheme| {
        let width = crate::ports::text_layout::terminal_cell_width(grapheme);
        let is_selected = selected
            .as_ref()
            .is_some_and(|range| column < range.end && column.saturating_add(width) > range.start);
        column = column.saturating_add(width);
        let style = if is_selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        Span::styled(grapheme.to_owned(), style)
    }));
    Line::from(spans).style(theme.focused_style())
}
