//! Deterministic one-column board renderer.

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

use crate::{
    application::{DurabilityState, InteractionMode},
    ports::text_layout::wrap_rows,
};

use super::{BoardApp, HitTarget, LayoutSnapshot, Theme, ThoughtLayout, layout::OverlayLayout};

/// Render the complete board into one terminal frame.
pub fn render(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    frame.render_widget(Block::default().style(theme.base_style()), layout.area);
    render_board(frame, app, layout, theme);
    render_footer(frame, app, layout, theme);
    if let Some((query, entries, selected)) = app.palette_view() {
        if let Some(overlay) = &layout.overlay {
            render_palette(frame, overlay, &query, &entries, selected, theme);
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
    if layout.thoughts.is_empty() {
        frame.render_widget(
            Paragraph::new("  +  create a thought with n or paste text")
                .style(Style::default().fg(theme.muted)),
            layout.board,
        );
    }
    for thought_layout in &layout.thoughts {
        let Some(thought) = app.state.board.thought(thought_layout.thought_id) else {
            continue;
        };
        let focused = app.state.focused_thought == Some(thought.id);
        let hovered = matches!(
            app.hovered(),
            Some(HitTarget::Thought(id) | HitTarget::DragHandle(id) | HitTarget::Overflow(id))
                if id == thought.id
        );
        render_gutter(frame, thought_layout, focused, hovered, theme);
        if matches!(app.state.mode, InteractionMode::Edit { thought_id } if thought_id == thought.id)
        {
            render_editor(frame, app, thought_layout, focused, theme);
        } else {
            render_thought(frame, &thought.content, thought_layout, theme);
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
        frame.render_widget(Paragraph::new("  +").style(style), insert);
    }
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

fn render_thought(frame: &mut Frame<'_>, content: &str, layout: &ThoughtLayout, theme: &Theme) {
    let content_rows =
        usize::from(layout.text_area.height).saturating_sub(usize::from(layout.overflow.is_some()));
    let lines = wrap_rows(content, usize::from(layout.text_area.width.max(1)))
        .into_iter()
        .take(content_rows)
        .map(|row| Line::raw(row.visual.text))
        .collect::<Vec<_>>();
    let mut paragraph = Paragraph::new(Text::from(lines));
    if layout.hidden_rows > 0 {
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
    layout: &ThoughtLayout,
    focused: bool,
    theme: &Theme,
) {
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
        Paragraph::new(visible).style(Style::default().fg(if focused {
            theme.accent
        } else {
            theme.foreground
        })),
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
            line.text
                .graphemes(true)
                .take(offset)
                .map(unicode_width::UnicodeWidthStr::width)
                .sum()
        })
}

fn render_footer(frame: &mut Frame<'_>, app: &BoardApp, layout: &LayoutSnapshot, theme: &Theme) {
    let durability = match app.state.durability {
        DurabilityState::Durable { .. } => "saved",
        DurabilityState::Pending { .. } => "saving",
        DurabilityState::Failed { .. } => "save failed  r retry  w recovery",
    };
    let mode = match app.state.mode {
        InteractionMode::Board => "board",
        InteractionMode::Edit { .. } => "edit",
    };
    let keys = app.keybindings();
    let text = app.status.as_deref().map_or_else(
        || {
            let base = format!(
                " {mode}  {durability}  {} new  enter edit  {} undo  {} help  {} quit",
                keys.new, keys.undo, keys.help, keys.quit
            );
            app.agent_hint()
                .map_or(base.clone(), |hint| format!(" {hint}  {base}"))
        },
        |status| format!(" {status}"),
    );
    let footer_color = if matches!(app.state.durability, DurabilityState::Failed { .. }) {
        theme.error
    } else {
        theme.muted
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(footer_color)),
        layout.footer,
    );
    for (target, area) in &layout.controls {
        let label = match target {
            HitTarget::Commands => format!("[{}]", keys.commands),
            HitTarget::Copy => format!("[{}]", keys.copy),
            HitTarget::Cut => format!("[{}]", keys.cut),
            HitTarget::Delete => format!("[{}]", keys.delete),
            HitTarget::Submit(direction, remove) => {
                let key = if *remove {
                    keys.submit_remove
                } else {
                    keys.submit
                };
                format!("{key}{}", direction_symbol(*direction))
            }
            HitTarget::Undo => format!("[{}]", keys.undo),
            HitTarget::Help => format!("[{}]", keys.help),
            HitTarget::Quit => format!("[{}]", keys.quit),
            _ => continue,
        };
        let style = if app.hovered() == Some(*target) {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default().fg(theme.accent)
        };
        frame.render_widget(Paragraph::new(label).style(style), *area);
    }
}

const fn direction_symbol(direction: crate::domain::Direction) -> &'static str {
    match direction {
        crate::domain::Direction::Up => "↑",
        crate::domain::Direction::Right => "→",
        crate::domain::Direction::Down => "↓",
        crate::domain::Direction::Left => "←",
    }
}

fn render_help(frame: &mut Frame<'_>, app: &BoardApp, overlay: &OverlayLayout, theme: &Theme) {
    let keys = app.keybindings();
    let content = format!(
        "Board\n  {} new   {}/{} focus   Enter/{} edit   {} delete\n  {}/{} move   {} undo   {} collapse   {}/{} submit   {} quit\n\nEdit\n  Esc board   Primary+A select all   Primary+U delete line\n  Primary+Z undo   Shift+Primary+Z redo\n\nPaste and file drop are one operation. Clipboard images become private PNG paths. Submission uses verified Herdr agents only.",
        keys.new,
        keys.focus_down,
        keys.focus_up,
        keys.edit,
        keys.delete,
        keys.move_down,
        keys.move_up,
        keys.undo,
        keys.collapse,
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

fn render_palette(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    query: &str,
    entries: &[&str],
    selected: usize,
    theme: &Theme,
) {
    frame.render_widget(Clear, overlay.area);
    frame.render_widget(
        Paragraph::new(format!("/{query}"))
            .block(Block::default().title(" commands ").borders(Borders::ALL)),
        overlay.area,
    );
    for (index, (entry, area)) in entries.iter().zip(&overlay.items).enumerate() {
        let style = if index == selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        frame.render_widget(Paragraph::new(*entry).style(style), *area);
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
