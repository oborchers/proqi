//! Deterministic one-column board renderer.

use ratatui_core::{
    layout::Rect,
    style::{Color, Modifier, Style},
    terminal::Frame,
    text::{Line, Span, Text},
};
use ratatui_widgets::{
    block::{Block, Padding},
    borders::Borders,
    clear::Clear,
    paragraph::{Paragraph, Wrap},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::application::{DurabilityState, InteractionMode};

use super::BoardApp;

const GREEN: Color = Color::Rgb(38, 112, 75);

/// Render the complete board into one terminal frame.
pub fn render(frame: &mut Frame<'_>, app: &BoardApp) {
    let area = frame.area();
    let footer_height = 1_u16;
    let board = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(footer_height),
    );
    let footer = Rect::new(
        area.x,
        area.bottom().saturating_sub(footer_height),
        area.width,
        footer_height.min(area.height),
    );
    render_board(frame, app, board);
    render_footer(frame, app, footer);
    if app.help {
        render_help(frame, area);
    }
}

fn render_board(frame: &mut Frame<'_>, app: &BoardApp, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let thoughts = app
        .state
        .board
        .live_thoughts()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    if thoughts.is_empty() {
        frame.render_widget(
            Paragraph::new("  +  create a thought with n or paste text")
                .style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }
    let mut y = area.y;
    for thought in thoughts {
        if y >= area.bottom() {
            break;
        }
        let focused = app.state.focused_thought == Some(thought.id);
        let available = area.bottom().saturating_sub(y);
        let natural = natural_height(&thought.content, area.width.saturating_sub(4));
        let cap = area.height.saturating_sub(2).max(3);
        let height = natural.min(cap).min(available).max(1);
        let rect = Rect::new(area.x, y, area.width, height);
        if matches!(app.state.mode, InteractionMode::Edit { thought_id } if thought_id == thought.id)
        {
            render_editor(frame, app, rect, focused);
        } else {
            render_thought(frame, &thought.content, thought.collapsed, rect, focused);
        }
        y = y.saturating_add(height);
    }
    if y < area.bottom() {
        frame.render_widget(
            Paragraph::new("  +").style(Style::default().fg(GREEN)),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

fn render_thought(
    frame: &mut Frame<'_>,
    content: &str,
    collapsed: bool,
    area: Rect,
    focused: bool,
) {
    let gutter = if focused { "│ " } else { "  " };
    let lines = thought_lines(content, gutter, focused);
    let block = Block::default().padding(Padding::new(1, 1, 0, 0));
    let mut paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false });
    if collapsed {
        paragraph = paragraph.style(Style::default().fg(Color::DarkGray));
    }
    frame.render_widget(paragraph, area);
}

fn render_editor(frame: &mut Frame<'_>, app: &BoardApp, area: Rect, focused: bool) {
    let Some(snapshot) = app.editor_snapshot() else {
        return;
    };
    let visible = snapshot
        .visual_lines
        .iter()
        .skip(snapshot.scroll_row)
        .take(usize::from(area.height))
        .map(|line| Line::raw(format!("│ {}", line.text)))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(visible).style(Style::default().fg(if focused {
            GREEN
        } else {
            Color::Reset
        })),
        area,
    );
    let cursor_row = snapshot
        .visual_lines
        .iter()
        .position(|line| {
            line.logical_line == snapshot.cursor.line
                && snapshot.cursor.grapheme >= line.start_grapheme
                && snapshot.cursor.grapheme <= line.end_grapheme
        })
        .unwrap_or(snapshot.scroll_row)
        .saturating_sub(snapshot.scroll_row);
    let cursor_column = snapshot
        .visual_lines
        .get(snapshot.scroll_row + cursor_row)
        .map_or(0, |line| {
            let offset = snapshot.cursor.grapheme.saturating_sub(line.start_grapheme);
            let prefix = line.text.graphemes(true).take(offset).collect::<String>();
            UnicodeWidthStr::width(prefix.as_str())
        });
    let x = area
        .x
        .saturating_add(2)
        .saturating_add(u16::try_from(cursor_column).unwrap_or(u16::MAX));
    let y = area
        .y
        .saturating_add(u16::try_from(cursor_row).unwrap_or(u16::MAX));
    if x < area.right() && y < area.bottom() {
        frame.set_cursor_position((x, y));
    }
}

fn thought_lines<'a>(content: &'a str, gutter: &'a str, focused: bool) -> Vec<Line<'a>> {
    let style = Style::default().fg(if focused { GREEN } else { Color::DarkGray });
    let mut lines = content
        .split('\n')
        .map(|line| Line::from(vec![Span::styled(gutter, style), Span::raw(line)]))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(gutter, style),
            Span::raw(" "),
        ]));
    }
    lines
}

fn render_footer(frame: &mut Frame<'_>, app: &BoardApp, area: Rect) {
    let durability = match app.state.durability {
        DurabilityState::Durable { .. } => "saved",
        DurabilityState::Pending { .. } => "saving",
        DurabilityState::Failed { .. } => "save failed",
    };
    let mode = match app.state.mode {
        InteractionMode::Board => "board",
        InteractionMode::Edit { .. } => "edit",
    };
    let text = app.status.as_deref().map_or_else(
        || format!(" {mode}  {durability}  n new  enter edit  u undo  ? help  q quit"),
        |status| format!(" {status}"),
    );
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    let width = area.width.clamp(1, 58);
    let height = area.height.clamp(1, 12);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(
            "Board\n  n new   j/k focus   Enter edit   d delete\n  J/K move   u undo   Space collapse   q quit\n\nEdit\n  Esc board   Ctrl+A select all   Ctrl+U delete line\n  Ctrl+Z undo   Ctrl+Y redo\n\nPaste is one exact operation.",
        )
        .block(
            Block::default()
                .title(Span::styled(" proqi help ", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false }),
        popup,
    );
}

fn natural_height(content: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let rows = content
        .split('\n')
        .map(|line| UnicodeWidthStr::width(line).max(1).div_ceil(width))
        .sum::<usize>();
    u16::try_from(rows.max(1)).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use ratatui_core::{buffer::Buffer, layout::Rect, widgets::Widget};
    use ratatui_widgets::paragraph::Paragraph;

    use super::{natural_height, thought_lines};

    #[test]
    fn multiline_thoughts_are_distinct_buffer_rows() {
        let area = Rect::new(0, 0, 12, 2);
        let mut buffer = Buffer::empty(area);
        Paragraph::new(thought_lines("first\n第二", "│ ", true)).render(area, &mut buffer);
        assert_eq!(buffer[(2, 0)].symbol(), "f");
        assert_eq!(buffer[(2, 1)].symbol(), "第");
    }

    #[test]
    fn natural_height_uses_terminal_cells_and_preserves_blank_lines() {
        assert_eq!(natural_height("界界", 2), 2);
        assert_eq!(natural_height("a\n\nb", 20), 3);
    }
}
