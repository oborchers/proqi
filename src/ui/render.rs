//! Deterministic one-column board renderer.

mod chrome;
mod overlay_composition;
mod overlays;
mod release_highlights;

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

use super::{
    BoardApp, HitTarget, LayoutSnapshot, Theme, ThoughtLayout, app::InvocationChoiceView,
    layout::OverlayLayout,
};

/// Render the complete board into one terminal frame.
pub fn render(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    let _release_highlights_visible = render_with_outcome(frame, app, layout, theme);
}

pub(crate) fn render_with_outcome(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    layout: &LayoutSnapshot,
    theme: &Theme,
) -> bool {
    frame.render_widget(Block::default().style(theme.base_style()), layout.area);
    render_board(frame, app, layout, theme);
    chrome::render_footer(frame, app, layout, theme);
    overlay_composition::render(frame, app, layout, theme)
}

pub(super) struct PlainPickerView {
    pub(super) title: &'static str,
    pub(super) prompt: char,
    pub(super) query: String,
    pub(super) entries: Vec<String>,
    pub(super) selected: usize,
}

pub(super) fn render_plain_picker(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    app: &BoardApp,
    picker: PlainPickerView,
    theme: &Theme,
) {
    let PlainPickerView {
        title,
        prompt,
        query,
        entries,
        selected,
    } = picker;
    let rows = entries
        .iter()
        .map(|entry| overlays::PickerRow::plain(entry))
        .collect::<Vec<_>>();
    overlays::render_picker(
        frame,
        overlay,
        overlays::PickerView {
            title,
            prompt,
            query: &query,
            cursor: app.overlay_query_cursor().unwrap_or(query.len()),
            entries: &rows,
            selected,
        },
        app.picker_overflow(overlay.items.len()),
        theme,
    );
}

pub(super) struct InvocationPickerView {
    pub(super) query: String,
    pub(super) entries: Vec<InvocationChoiceView>,
    pub(super) selected: usize,
    pub(super) notice: Option<&'static str>,
}

pub(super) fn render_invocation_picker(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    app: &BoardApp,
    picker: InvocationPickerView,
    theme: &Theme,
) {
    let InvocationPickerView {
        query,
        entries,
        selected,
        notice,
    } = picker;
    let rows = entries
        .iter()
        .map(|entry| {
            overlays::PickerRow::grouped(
                &entry.token,
                &entry.qualifier,
                &entry.qualifier_fallbacks,
                entry.group.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    overlays::render_picker(
        frame,
        overlay,
        overlays::PickerView {
            title: notice.unwrap_or(" discovered invocations "),
            prompt: '›',
            query: &query,
            cursor: app.overlay_query_cursor().unwrap_or(query.len()),
            entries: &rows,
            selected,
        },
        app.picker_overflow(overlay.items.len()),
        theme,
    );
}

fn render_board(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    if layout.board.width == 0 || layout.board.height == 0 {
        return;
    }
    let presentation = app.presentation_for_layout(layout);
    let editor = presentation.editor();
    for thought_layout in &layout.thoughts {
        let Some(thought) = presentation.thought(thought_layout.thought_id) else {
            continue;
        };
        let focused = app.active_thought_id() == Some(thought_layout.thought_id);
        let selected = app.thought_selected(thought_layout.thought_id);
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
        if focused || hovered || selected {
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
            render_editor(frame, app, editor, thought_layout.text_area, theme);
        } else {
            render_thought(
                frame,
                app,
                &thought.presentation,
                thought_layout,
                focused || selected,
                theme,
            );
        }
    }
    if let Some(compose) = &layout.compose {
        frame.render_widget(Block::default().style(theme.focused_style()), compose.area);
        render_compose_gutter(frame, compose, theme);
        render_editor(frame, app, editor, compose.text_area, theme);
    }
    if let Some(insert) = layout.insert {
        let hovered = app.hovered() == Some(HitTarget::Insert);
        let prompt = app.compose_prompt_visible();
        let label = Line::from(vec![
            Span::styled("+", Style::default().fg(theme.accent)),
            Span::styled(
                crate::ui::control_labels::insertion_text(
                    app.interaction_mode(),
                    insert.width < 14,
                ),
                Style::default().fg(theme.foreground),
            ),
        ]);
        let style = if !prompt && (hovered || app.insertion_focused()) {
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

fn render_compose_gutter(
    frame: &mut Frame<'_>,
    layout: &crate::ui::layout::ComposeLayout,
    theme: &Theme,
) {
    let padding = usize::from(layout.gutter.height.saturating_sub(1) / 2);
    let content = format!("{}⋮", "\n".repeat(padding));
    frame.render_widget(
        Paragraph::new(content).style(
            Style::default()
                .fg(theme.on_accent)
                .bg(theme.accent_surface)
                .remove_modifier(Modifier::REVERSED)
                .add_modifier(Modifier::BOLD),
        ),
        layout.gutter,
    );
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
    app: &BoardApp,
    presentation: &crate::ui::annotations::Presentation,
    layout: &ThoughtLayout,
    focused: bool,
    theme: &Theme,
) {
    let links = url_ranges(&presentation.content);
    let invocations = app.invocation_ranges(&presentation.content);
    let content_rows =
        usize::from(layout.text_area.height).saturating_sub(usize::from(layout.overflow.is_some()));
    let rendered_lines = wrap_rows(
        &presentation.content,
        usize::from(layout.text_area.width.max(1)),
    )
    .into_iter()
    .skip(layout.content_row_offset)
    .take(content_rows)
    .map(|row| {
        styled_line(
            &presentation.content,
            &row.visual,
            &presentation.styles,
            &links,
            &invocations,
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

fn render_editor(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    presentation: Option<&crate::ui::projection::EditorPresentation>,
    text_area: ratatui_core::layout::Rect,
    theme: &Theme,
) {
    let Some(presentation) = presentation else {
        return;
    };
    let snapshot = &presentation.snapshot;
    let links = url_ranges(&snapshot.content);
    let invocations = app.invocation_ranges(&snapshot.content);
    let visible = snapshot
        .visual_lines
        .iter()
        .skip(snapshot.scroll_row)
        .take(usize::from(text_area.height))
        .map(|line| {
            styled_line(
                &snapshot.content,
                line,
                &presentation.styles,
                &links,
                &invocations,
                theme,
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(visible).style(Style::default().fg(theme.foreground)),
        text_area,
    );
    let Some((cursor_column, cursor_row)) = presentation.cursor_viewport_cell() else {
        return;
    };
    let x = text_area
        .x
        .saturating_add(u16::try_from(cursor_column).unwrap_or(u16::MAX));
    let y = text_area
        .y
        .saturating_add(u16::try_from(cursor_row).unwrap_or(u16::MAX));
    if x < text_area.right() && y < text_area.bottom() {
        frame.set_cursor_position((x, y));
    }
}

fn styled_line(
    content: &str,
    line: &crate::ports::editor::VisualLine,
    semantic_styles: &[crate::ui::annotations::PresentedStyle],
    links: &[std::ops::Range<usize>],
    invocations: &[std::ops::Range<usize>],
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
            let semantic = semantic_styles
                .iter()
                .find(|style| byte >= style.start && byte < style.end)
                .map(|style| style.kind);
            let linked = links.iter().any(|range| range.contains(&byte));
            let invocation = invocations.iter().any(|range| range.contains(&byte));
            column = column.saturating_add(width);
            let mut style = Style::default().fg(
                if matches!(
                    semantic,
                    Some(crate::ui::annotations::PresentedStyleKind::Warning)
                ) {
                    theme.warning
                } else if semantic.is_some() || invocation {
                    theme.annotation
                } else if linked {
                    theme.link
                } else {
                    theme.foreground
                },
            );
            if semantic.is_some() || invocation {
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
    crate::ports::text_layout::display_grapheme(grapheme, column)
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
