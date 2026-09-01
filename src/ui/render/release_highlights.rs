//! Release-highlight overlay rendering.

use ratatui_core::{
    style::{Modifier, Style},
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::{
    block::Block,
    borders::Borders,
    paragraph::{Paragraph, Wrap},
};

use crate::ui::{BoardApp, Theme, app::highlights::ReleaseHighlightRow, layout::OverlayLayout};

use super::overlays::{clear_overlay, ellipsize, render_close};

pub(super) fn render(
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
    super::overlays::render_overflow_cues(
        frame,
        overlay,
        (view.overflow_above, view.overflow_below),
        theme,
    );
    render_close(frame, overlay, theme);
}

fn highlight_line(row: ReleaseHighlightRow, theme: &Theme) -> Line<'static> {
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
