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
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

use super::super::{BoardApp, Theme, layout::OverlayLayout};

pub(super) fn render_release_highlights(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    overlay: &OverlayLayout,
    theme: &Theme,
) {
    let content_width = overlay.area.width.saturating_sub(2);
    let content_height = overlay.area.height.saturating_sub(2);
    let Some(view) = app.release_highlights_view(content_width, content_height) else {
        return;
    };
    let title = ellipsize(
        &view.title,
        usize::from(overlay.area.width.saturating_sub(5)),
    );
    let lines = view
        .rows
        .into_iter()
        .skip(view.scroll)
        .take(usize::from(content_height))
        .map(|row| highlight_line(row, theme))
        .collect::<Vec<_>>();
    clear_overlay(frame, overlay.area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(Span::styled(
                        title,
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .style(theme.base_style())
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        overlay.area,
    );
    render_close(frame, overlay, theme);
}

fn highlight_line(
    row: crate::ui::app::highlights::ReleaseHighlightRow,
    theme: &Theme,
) -> Line<'static> {
    use crate::ui::app::highlights::ReleaseHighlightRow;
    match row {
        ReleaseHighlightRow::Version(version) => Line::from(Span::styled(
            version,
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        ReleaseHighlightRow::Bullet(text) => Line::from(vec![
            Span::styled("• ", Style::default().fg(theme.accent)),
            Span::styled(text, Style::default().fg(theme.foreground)),
        ]),
        ReleaseHighlightRow::Continuation(text) => Line::from(vec![
            Span::raw("  "),
            Span::styled(text, Style::default().fg(theme.foreground)),
        ]),
        ReleaseHighlightRow::Spacer => Line::default(),
    }
}

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
    render_close(frame, overlay, theme);
}

pub(super) fn render_picker(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    picker: PickerView<'_>,
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
    let available = input.width.saturating_sub(1);
    let (query, cursor) = visible_query(picker.query, picker.cursor, available);
    frame.render_widget(
        Paragraph::new(format!("{}{query}", picker.prompt)).style(theme.focused_style()),
        input,
    );
    frame.set_cursor_position((input.x.saturating_add(1).saturating_add(cursor), input.y));
    for (index, (entry, area)) in picker.entries.iter().zip(&overlay.items).enumerate() {
        let style = if index == picker.selected {
            theme
                .focused_style()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.base_style()
        };
        frame.render_widget(
            Paragraph::new(picker_row(*entry, area.width)).style(style),
            *area,
        );
    }
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
    frame.render_widget(
        Paragraph::new(format!("> {value}")).style(theme.focused_style()),
        input_area(overlay),
    );
    render_close(frame, overlay, theme);
    let x = overlay
        .area
        .x
        .saturating_add(3)
        .saturating_add(u16::try_from(value.width()).unwrap_or(u16::MAX))
        .min(overlay.area.right().saturating_sub(2));
    frame.set_cursor_position((x, overlay.area.y.saturating_add(1)));
}

fn clear_overlay(frame: &mut Frame<'_>, area: ratatui_core::layout::Rect) {
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
}

impl<'a> PickerRow<'a> {
    pub(super) const fn plain(primary: &'a str) -> Self {
        Self {
            primary,
            secondary: None,
        }
    }

    pub(super) const fn fields(primary: &'a str, secondary: &'a str) -> Self {
        Self {
            primary,
            secondary: Some(secondary),
        }
    }
}

fn picker_row(entry: PickerRow<'_>, width: u16) -> String {
    let width = usize::from(width);
    let primary_width = entry.primary.width();
    if let Some(secondary) = entry.secondary {
        let secondary_width = secondary.width();
        if primary_width
            .saturating_add(2)
            .saturating_add(secondary_width)
            <= width
        {
            let gap = width.saturating_sub(primary_width + secondary_width);
            return format!("{}{}{secondary}", entry.primary, " ".repeat(gap));
        }
    }
    ellipsize(entry.primary, width)
}

fn ellipsize(value: &str, width: usize) -> String {
    crate::ports::text_layout::ellipsize_cells(value, width)
}

fn visible_query(query: &str, cursor: usize, width: u16) -> (String, u16) {
    let cursor = cursor.min(query.len());
    let prefix = &query[..cursor];
    let mut start = 0;
    while prefix[start..].width() > usize::from(width) {
        start = prefix[start..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(cursor, |(offset, _)| start + offset);
    }
    let mut end = query.len();
    while query[start..end].width() > usize::from(width) {
        end = query[..end]
            .grapheme_indices(true)
            .next_back()
            .map_or(start, |(index, _)| index);
    }
    (
        query[start..end].to_owned(),
        u16::try_from(query[start..cursor].width()).unwrap_or(width),
    )
}

fn render_close(frame: &mut Frame<'_>, overlay: &OverlayLayout, theme: &Theme) {
    frame.render_widget(
        Paragraph::new("[x]").style(Style::default().fg(theme.accent)),
        overlay.close,
    );
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
    cell_width: usize,
    key_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (key, label)) in items.iter().enumerate() {
        spans.push(Span::styled(key.clone(), Style::default().fg(theme.accent)));
        spans.push(Span::raw(
            " ".repeat(key_width.saturating_sub(key.width()) + 1),
        ));
        spans.push(Span::styled(*label, Style::default().fg(theme.foreground)));
        if index + 1 < columns {
            let used = key_width + 1 + label.width();
            spans.push(Span::raw(" ".repeat(cell_width.saturating_sub(used))));
        }
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::{PickerRow, overlay_clear_area, picker_row};
    use ratatui_core::layout::Rect;
    use unicode_width::UnicodeWidthStr as _;

    #[test]
    fn two_field_row_right_aligns_qualifier() {
        assert_eq!(
            picker_row(PickerRow::fields("$skill", "Global Skill"), 24),
            "$skill      Global Skill"
        );
    }

    #[test]
    fn narrow_row_hides_qualifier_before_token() {
        assert_eq!(
            picker_row(PickerRow::fields("$long-skill", "Global Skill"), 11),
            "$long-skill"
        );
    }

    #[test]
    fn overlong_token_ellipsizes_on_grapheme_and_cell_boundaries() {
        let rendered = picker_row(PickerRow::fields("$界界e\u{301}🙂", "Global Skill"), 5);

        assert_eq!(rendered, "$界…");
        assert!(rendered.width() <= 5);
    }

    #[test]
    fn overlay_clear_halo_clamps_to_the_viewport() {
        let viewport = Rect::new(4, 2, 20, 8);
        assert_eq!(
            overlay_clear_area(viewport, Rect::new(8, 3, 10, 4)),
            Rect::new(7, 3, 12, 4)
        );
        assert_eq!(
            overlay_clear_area(viewport, Rect::new(4, 2, 20, 8)),
            viewport
        );
    }
}
