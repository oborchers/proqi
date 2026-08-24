//! Deterministic one-column board renderer.

mod chrome;
mod overlays;

use linkify::{LinkFinder, LinkKind};
use ratatui_core::{
    layout::Alignment,
    style::{Modifier, Style},
    terminal::Frame,
    text::{Line, Span, Text},
};
use ratatui_widgets::{block::Block, clear::Clear, paragraph::Paragraph};
use unicode_segmentation::UnicodeSegmentation;

use crate::{application::InteractionMode, ports::text_layout::wrap_rows};

use super::{BoardApp, HitTarget, LayoutSnapshot, Theme, ThoughtLayout};

/// Render the complete board into one terminal frame.
pub fn render(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    frame.render_widget(Block::default().style(theme.base_style()), layout.area);
    render_board(frame, app, layout, theme);
    chrome::render_footer(frame, app, layout, theme);
    if let Some((query, entries, selected)) = app.search_view() {
        if let Some(overlay) = &layout.overlay {
            overlays::render_picker(
                frame,
                overlay,
                overlays::PickerView {
                    title: " thoughts ",
                    prompt: '/',
                    query: &query,
                    entries: &entries,
                    selected,
                },
                theme,
            );
        }
    } else if let Some((query, entries, selected)) = app.session_transfer_view() {
        if let Some(overlay) = &layout.overlay {
            overlays::render_picker(
                frame,
                overlay,
                overlays::PickerView {
                    title: " send to Proqi session ",
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
            overlays::render_picker(
                frame,
                overlay,
                overlays::PickerView {
                    title: " commands ",
                    prompt: ':',
                    query: &query,
                    entries: &entries,
                    selected,
                },
                theme,
            );
        }
    } else if let Some(value) = app.session_rename_view() {
        if let Some(overlay) = &layout.overlay {
            overlays::render_text_prompt(frame, overlay, " rename session ", value, theme);
        }
    } else if app.help
        && let Some(overlay) = &layout.overlay
    {
        overlays::render_help(frame, app, overlay, theme);
    }
}

fn render_board(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    if layout.board.width == 0 || layout.board.height == 0 {
        return;
    }
    for thought_layout in &layout.thoughts {
        let Some(presentation) = app.presentation_for_render(thought_layout.thought_id) else {
            continue;
        };
        let focused = app.active_thought_id() == Some(thought_layout.thought_id);
        let hovered = matches!(
            app.hovered(),
            Some(HitTarget::Thought(id) | HitTarget::DragHandle(id) | HitTarget::Overflow(id))
                if id == thought_layout.thought_id
        );
        render_separator(
            frame,
            thought_layout,
            app.drag_target() == Some(thought_layout.index),
            theme,
        );
        if focused || hovered {
            frame.render_widget(
                Block::default().style(theme.focused_style()),
                thought_layout.area,
            );
        }
        render_gutter(
            frame,
            thought_layout,
            focused,
            hovered,
            app.dragged_thought() == Some(thought_layout.thought_id),
            theme,
        );
        if matches!(app.interaction_mode(), InteractionMode::Edit { thought_id } if thought_id == thought_layout.thought_id)
        {
            render_editor(frame, app, thought_layout, theme);
        } else {
            render_thought(frame, &presentation, thought_layout, focused, theme);
        }
    }
    if let Some(insert) = layout.insert {
        let hovered = app.hovered() == Some(HitTarget::Insert);
        let label = Line::from(vec![
            Span::styled("+", Style::default().fg(theme.accent)),
            Span::styled(" New thought", Style::default().fg(theme.foreground)),
        ]);
        let style = if hovered || app.insertion_focused() {
            theme.focused_style()
        } else {
            theme.base_style()
        };
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(style),
            insert,
        );
    }
}

fn render_separator(
    frame: &mut Frame<'_>,
    layout: &ThoughtLayout,
    drag_target: bool,
    theme: &Theme,
) {
    let Some(area) = layout.separator_before else {
        return;
    };
    frame.render_widget(
        Paragraph::new("─".repeat(usize::from(area.width))).style(Style::default().fg(
            if drag_target {
                theme.accent
            } else {
                theme.divider
            },
        )),
        area,
    );
}

fn render_gutter(
    frame: &mut Frame<'_>,
    layout: &ThoughtLayout,
    focused: bool,
    hovered: bool,
    dragging: bool,
    theme: &Theme,
) {
    let symbol = if focused || hovered { "⋮" } else { " " };
    let padding = usize::from(layout.gutter.height.saturating_sub(1) / 2);
    let content = format!("{}{symbol}", "\n".repeat(padding));
    let style = if focused {
        Style::default()
            .fg(theme.on_accent)
            .bg(theme.accent_surface)
            .remove_modifier(Modifier::REVERSED)
            .add_modifier(if dragging {
                Modifier::DIM
            } else {
                Modifier::BOLD
            })
    } else {
        Style::default().fg(theme.accent)
    };
    frame.render_widget(Paragraph::new(content).style(style), layout.gutter);
}

fn render_thought(
    frame: &mut Frame<'_>,
    presentation: &crate::ui::annotations::Presentation,
    layout: &ThoughtLayout,
    focused: bool,
    theme: &Theme,
) {
    let links = url_ranges(&presentation.content);
    let content_rows =
        usize::from(layout.text_area.height).saturating_sub(usize::from(layout.overflow.is_some()));
    let rendered_lines = wrap_rows(
        &presentation.content,
        usize::from(layout.text_area.width.max(1)),
    )
    .into_iter()
    .take(content_rows)
    .map(|row| {
        styled_line(
            &presentation.content,
            &row.visual,
            &presentation.folds,
            &links,
            theme,
        )
    })
    .collect::<Vec<_>>();
    let mut paragraph = Paragraph::new(Text::from(rendered_lines));
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
    let Some(presentation) = app.editor_presentation() else {
        return;
    };
    let snapshot = &presentation.snapshot;
    let links = url_ranges(&snapshot.content);
    let visible = snapshot
        .visual_lines
        .iter()
        .skip(snapshot.scroll_row)
        .take(usize::from(layout.text_area.height))
        .map(|line| styled_line(&snapshot.content, line, &presentation.folds, &links, theme))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(visible).style(Style::default().fg(theme.foreground)),
        layout.text_area,
    );
    let Some((cursor_column, cursor_row)) = presentation.cursor_viewport_cell() else {
        return;
    };
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

fn styled_line(
    content: &str,
    line: &crate::ports::editor::VisualLine,
    folds: &[crate::ui::annotations::PresentedFold],
    links: &[std::ops::Range<usize>],
    theme: &Theme,
) -> Line<'static> {
    let source = content
        .get(line.start_byte..line.end_byte)
        .unwrap_or_default();
    let mut column = 0;
    let spans = source
        .grapheme_indices(true)
        .map(|(offset, grapheme)| {
            let byte = line.start_byte.saturating_add(offset);
            let (visible, width) = visible_grapheme(grapheme, column);
            let selected = line.selected_cells.is_some_and(|selection| {
                column < selection.end && column.saturating_add(width) > selection.start
            });
            let folded = folds
                .iter()
                .any(|fold| fold.collapsed && byte >= fold.start && byte < fold.end);
            let linked = links.iter().any(|range| range.contains(&byte));
            column = column.saturating_add(width);
            let mut style = Style::default().fg(if folded || linked {
                theme.accent
            } else {
                theme.foreground
            });
            if folded {
                style = style.add_modifier(Modifier::BOLD);
            }
            if linked {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if selected {
                style = style.add_modifier(Modifier::REVERSED);
            }
            Span::styled(visible, style)
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

fn url_ranges(content: &str) -> Vec<std::ops::Range<usize>> {
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    finder
        .links(content)
        .filter(|link| {
            let value = link.as_str();
            value
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
                || value
                    .get(..8)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        })
        .map(|link| link.start()..link.end())
        .collect()
}

fn visible_grapheme(grapheme: &str, column: usize) -> (String, usize) {
    if grapheme == "\t" {
        let width = 4 - column % 4;
        (" ".repeat(width), width)
    } else if grapheme.chars().any(char::is_control) {
        ("�".to_owned(), 1)
    } else {
        (
            grapheme.to_owned(),
            unicode_width::UnicodeWidthStr::width(grapheme),
        )
    }
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
