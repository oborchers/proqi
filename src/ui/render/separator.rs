//! Rendering of durable payload-free separator items from prepared geometry.

use ratatui_core::{
    style::{Modifier, Style},
    terminal::Frame,
};
use ratatui_widgets::{block::Block, paragraph::Paragraph};

use crate::{
    domain::BoardItemId,
    ui::{BoardApp, HitTarget, Theme, ThoughtLayout, layout::SeparatorLayout},
};

pub(super) fn render_automatic(
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

pub(super) fn render(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    layout: &SeparatorLayout,
    theme: &Theme,
) {
    let item_id = BoardItemId::Separator(layout.separator_id);
    let focused = app.state.focused_item == Some(item_id) && !app.insertion_focused();
    let selected = app.item_selected(item_id);
    let hovered = matches!(
        app.hovered(),
        Some(HitTarget::Separator(id) | HitTarget::SeparatorDragHandle(id))
            if id == layout.separator_id
    );
    let dragging = app.dragged_item() == Some(item_id);
    if focused || hovered || selected {
        frame.render_widget(Block::default().style(theme.focused_style()), layout.area);
    }
    if let Some(line) = layout.line {
        let emphasized =
            focused || selected || hovered || dragging || app.drag_target() == Some(layout.index);
        let style = Style::default()
            .fg(if emphasized {
                theme.accent
            } else {
                theme.muted
            })
            .add_modifier(if focused || selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        frame.render_widget(
            Paragraph::new("━".repeat(usize::from(line.width))).style(style),
            line,
        );
    }
    let symbol = if focused || hovered { "⋮" } else { " " };
    let padding = usize::from(layout.gutter.height.saturating_sub(1) / 2);
    let style = if focused {
        Style::default()
            .fg(theme.on_accent)
            .bg(theme.accent_surface)
            .add_modifier(if dragging {
                Modifier::DIM
            } else {
                Modifier::BOLD
            })
    } else {
        Style::default().fg(theme.accent)
    };
    frame.render_widget(
        Paragraph::new(format!("{}{symbol}", "\n".repeat(padding))).style(style),
        layout.gutter,
    );
}
