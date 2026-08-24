//! Deterministic one-column board renderer.

mod chrome;

use ratatui_core::{
    style::{Modifier, Style},
    terminal::Frame,
    text::{Line, Span, Text},
};
use ratatui_widgets::{
    block::Block,
    borders::Borders,
    clear::Clear,
    paragraph::{Paragraph, Wrap},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{application::InteractionMode, ports::text_layout::wrap_rows};

use super::{BoardApp, HitTarget, LayoutSnapshot, Theme, ThoughtLayout, layout::OverlayLayout};

/// Render the complete board into one terminal frame.
pub fn render(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    frame.render_widget(Block::default().style(theme.base_style()), layout.area);
    chrome::render_header(frame, app, layout, theme);
    render_board(frame, app, layout, theme);
    chrome::render_footer(frame, app, layout, theme);
    if let Some((query, entries, selected)) = app.search_view() {
        if let Some(overlay) = &layout.overlay {
            render_picker(
                frame,
                overlay,
                &PickerView {
                    title: " thoughts ",
                    prompt: '/',
                    query: &query,
                    entries: &entries,
                    selected,
                },
                theme,
            );
        }
    } else if let Some((query, entries, selected)) = app.palette_view() {
        if let Some(overlay) = &layout.overlay {
            render_picker(
                frame,
                overlay,
                &PickerView {
                    title: " commands ",
                    prompt: ':',
                    query: &query,
                    entries: &entries,
                    selected,
                },
                theme,
            );
        }
    } else if app.help
        && let Some(overlay) = &layout.overlay
    {
        render_help(frame, app, overlay, theme);
    }
}

fn render_board(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    if layout.board.width == 0 || layout.board.height == 0 {
        return;
    }
    for thought_layout in &layout.thoughts {
        let Some(content) = app.content_for_render(thought_layout.thought_id) else {
            continue;
        };
        let focused = app.active_thought_id() == Some(thought_layout.thought_id);
        let hovered = matches!(
            app.hovered(),
            Some(HitTarget::Thought(id) | HitTarget::DragHandle(id) | HitTarget::Overflow(id))
                if id == thought_layout.thought_id
        );
        render_separator(frame, thought_layout, theme);
        if focused {
            frame.render_widget(
                Block::default().style(theme.focused_style()),
                thought_layout.area,
            );
        }
        render_gutter(frame, thought_layout, focused, hovered, theme);
        if matches!(app.interaction_mode(), InteractionMode::Edit { thought_id } if thought_id == thought_layout.thought_id)
        {
            render_editor(frame, app, thought_layout, theme);
        } else {
            render_thought(frame, &content, thought_layout, focused, theme);
        }
    }
    if let Some(insert) = layout.insert {
        let style = if app.hovered() == Some(HitTarget::Insert) {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme.accent)
        };
        let label =
            if app.hovered() == Some(HitTarget::Insert) || app.state.focused_thought.is_none() {
                "  + New thought"
            } else {
                "  +"
            };
        frame.render_widget(Paragraph::new(label).style(style), insert);
    }
}

fn render_separator(frame: &mut Frame<'_>, layout: &ThoughtLayout, theme: &Theme) {
    let Some(area) = layout.separator_before else {
        return;
    };
    frame.render_widget(
        Paragraph::new("─".repeat(usize::from(area.width)))
            .style(Style::default().fg(theme.divider)),
        area,
    );
}

fn render_gutter(
    frame: &mut Frame<'_>,
    layout: &ThoughtLayout,
    focused: bool,
    hovered: bool,
    theme: &Theme,
) {
    let symbol = if focused {
        "│"
    } else if hovered {
        "┆"
    } else {
        " "
    };
    frame.render_widget(
        Paragraph::new(symbol).style(Style::default().fg(if focused || hovered {
            theme.accent
        } else {
            theme.muted
        })),
        layout.gutter,
    );
}

fn render_thought(
    frame: &mut Frame<'_>,
    content: &str,
    layout: &ThoughtLayout,
    focused: bool,
    theme: &Theme,
) {
    let content_rows =
        usize::from(layout.text_area.height).saturating_sub(usize::from(layout.overflow.is_some()));
    let lines = wrap_rows(content, usize::from(layout.text_area.width.max(1)))
        .into_iter()
        .take(content_rows)
        .map(|row| Line::raw(row.visual.text))
        .collect::<Vec<_>>();
    let mut paragraph = Paragraph::new(Text::from(lines));
    if focused {
        paragraph = paragraph.style(Style::default().fg(theme.foreground));
    } else if layout.hidden_rows > 0 {
        paragraph = paragraph.style(Style::default().fg(theme.muted));
    }
    frame.render_widget(paragraph, layout.text_area);
    if let Some(overflow) = layout.overflow {
        frame.render_widget(Clear, overflow);
        frame.render_widget(
            Paragraph::new(format!("{} more lines  expand", layout.hidden_rows)).style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::DIM),
            ),
            overflow,
        );
    }
}

fn render_editor(frame: &mut Frame<'_>, app: &BoardApp, layout: &ThoughtLayout, theme: &Theme) {
    let Some(snapshot) = app.editor_snapshot() else {
        return;
    };
    let visible = snapshot
        .visual_lines
        .iter()
        .skip(snapshot.scroll_row)
        .take(usize::from(layout.text_area.height))
        .map(|line| editor_line(line, theme))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(visible).style(Style::default().fg(theme.foreground)),
        layout.text_area,
    );
    let cursor_row = cursor_visual_row(&snapshot)
        .unwrap_or(snapshot.scroll_row)
        .saturating_sub(snapshot.scroll_row);
    let cursor_column = cursor_column(&snapshot, cursor_row);
    let x = layout
        .text_area
        .x
        .saturating_add(u16::try_from(cursor_column).unwrap_or(u16::MAX));
    let y = layout
        .text_area
        .y
        .saturating_add(u16::try_from(cursor_row).unwrap_or(u16::MAX));
    if x < layout.text_area.right() && y < layout.text_area.bottom() {
        frame.set_cursor_position((x, y));
    }
}

fn cursor_visual_row(snapshot: &crate::ports::editor::EditorSnapshot) -> Option<usize> {
    snapshot
        .visual_lines
        .iter()
        .enumerate()
        .find_map(|(index, line)| {
            if line.logical_line != snapshot.cursor.line
                || snapshot.cursor.grapheme < line.start_grapheme
            {
                return None;
            }
            let next_same_line = snapshot
                .visual_lines
                .get(index + 1)
                .is_some_and(|next| next.logical_line == line.logical_line);
            (snapshot.cursor.grapheme < line.end_grapheme
                || (snapshot.cursor.grapheme == line.end_grapheme && !next_same_line))
                .then_some(index)
        })
}

fn editor_line(line: &crate::ports::editor::VisualLine, theme: &Theme) -> Line<'static> {
    let Some(selection) = line.selected_cells else {
        return Line::raw(line.text.clone());
    };
    let mut column = 0;
    let spans = line
        .text
        .graphemes(true)
        .map(|grapheme| {
            let width = unicode_width::UnicodeWidthStr::width(grapheme);
            let selected = column < selection.end && column.saturating_add(width) > selection.start;
            column = column.saturating_add(width);
            if selected {
                Span::styled(
                    grapheme.to_owned(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::REVERSED),
                )
            } else {
                Span::raw(grapheme.to_owned())
            }
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

fn cursor_column(snapshot: &crate::ports::editor::EditorSnapshot, cursor_row: usize) -> usize {
    snapshot
        .visual_lines
        .get(snapshot.scroll_row + cursor_row)
        .map_or(0, |line| {
            let offset = snapshot.cursor.grapheme.saturating_sub(line.start_grapheme);
            let logical = snapshot
                .content
                .split('\n')
                .nth(line.logical_line)
                .unwrap_or_default()
                .graphemes(true)
                .skip(line.start_grapheme)
                .take(offset)
                .collect::<String>();
            crate::ports::text_layout::display_width(&logical)
        })
}

fn render_help(frame: &mut Frame<'_>, app: &BoardApp, overlay: &OverlayLayout, theme: &Theme) {
    let keys = app.keybindings();
    let content = format!(
        "Board\n  {} new   {}/{} focus   Enter/{} edit   {} delete\n  {}/{} move   {} undo   {} collapse   {} search   {} commands\n  {}/{} submit   {} quit\n\nEdit\n  Esc board   Primary+A select all   Primary+U delete line\n  Primary+Z undo   Shift+Primary+Z redo\n\nPaste and file drop are one operation. Clipboard images become private PNG paths. Submission uses verified Herdr agents only.",
        keys.new,
        keys.focus_down,
        keys.focus_up,
        keys.edit,
        keys.delete,
        keys.move_down,
        keys.move_up,
        keys.undo,
        keys.collapse,
        keys.search,
        keys.commands,
        keys.submit,
        keys.submit_remove,
        keys.quit,
    );
    frame.render_widget(Clear, overlay.area);
    frame.render_widget(
        Paragraph::new(content)
            .block(
                Block::default()
                    .title(Span::styled(
                        " proqi help ",
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: false }),
        overlay.area,
    );
    frame.render_widget(
        Paragraph::new("[x]").style(Style::default().fg(theme.accent)),
        overlay.close,
    );
}

struct PickerView<'a> {
    title: &'a str,
    prompt: char,
    query: &'a str,
    entries: &'a [String],
    selected: usize,
}

fn render_picker(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    picker: &PickerView<'_>,
    theme: &Theme,
) {
    frame.render_widget(Clear, overlay.area);
    frame.render_widget(
        Paragraph::new(format!("{}{query}", picker.prompt, query = picker.query))
            .block(Block::default().title(picker.title).borders(Borders::ALL)),
        overlay.area,
    );
    for (index, (entry, area)) in picker.entries.iter().zip(&overlay.items).enumerate() {
        let style = if index == picker.selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        frame.render_widget(Paragraph::new(entry.as_str()).style(style), *area);
    }
    frame.render_widget(
        Paragraph::new("[x]").style(Style::default().fg(theme.accent)),
        overlay.close,
    );
}

#[cfg(test)]
mod tests {
    use ratatui_core::{buffer::Buffer, layout::Rect, text::Line, widgets::Widget};
    use ratatui_widgets::paragraph::Paragraph;

    use crate::ports::text_layout::wrap_rows;
    #[test]
    fn multiline_thoughts_are_distinct_buffer_rows() {
        let area = Rect::new(0, 0, 12, 2);
        let mut buffer = Buffer::empty(area);
        let lines = wrap_rows("first\n第二", 12)
            .into_iter()
            .map(|row| Line::raw(row.visual.text))
            .collect::<Vec<_>>();
        Paragraph::new(lines).render(area, &mut buffer);
        assert_eq!(buffer[(0, 0)].symbol(), "f");
        assert_eq!(buffer[(0, 1)].symbol(), "第");
    }
}
