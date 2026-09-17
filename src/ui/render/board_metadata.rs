//! Optional names and separators around canonical thought bodies.

use ratatui_core::{
    style::{Modifier, Style},
    terminal::Frame,
};
use ratatui_widgets::paragraph::Paragraph;

use crate::ui::{BoardApp, HitTarget, Theme, ThoughtLayout};

pub(super) fn render_thought_name(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    thought: &crate::ui::projection::PresentedThought,
    layout: &ThoughtLayout,
    theme: &Theme,
) {
    let Some(area) = layout.name else {
        return;
    };
    let hovered = app.hovered() == Some(HitTarget::ThoughtName(layout.thought_id));
    let style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD)
        .add_modifier(if hovered {
            Modifier::UNDERLINED
        } else {
            Modifier::empty()
        });
    if let Some(editor) = app.thought_name_editor(layout.thought_id) {
        let width = usize::from(area.width);
        let window = crate::ports::text_layout::visible_cell_window(
            editor.text(),
            editor.cursor(),
            width.saturating_sub(1),
        );
        let line = crate::ui::query_render::input_line(
            "",
            editor.text(),
            &window,
            editor.selection(),
            theme,
        )
        .style(style);
        frame.render_widget(Paragraph::new(line), area);
        let cursor = crate::ports::text_layout::cursor_cell(editor.text(), editor.cursor())
            .saturating_sub(window.start_cell);
        let x = area
            .x
            .saturating_add(u16::try_from(cursor).unwrap_or(u16::MAX));
        if x < area.right() {
            frame.set_cursor_position((x, area.y));
        }
    } else if let Some(name) = thought.name.as_deref() {
        let text = crate::ports::text_layout::ellipsize_cells(name, usize::from(area.width));
        frame.render_widget(Paragraph::new(text).style(style), area);
    }
}

pub(super) fn render_separator(
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
