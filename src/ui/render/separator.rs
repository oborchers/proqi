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
    let body_hovered = matches!(
        app.hovered(),
        Some(HitTarget::Separator(id)) if id == layout.separator_id
    );
    let gutter_hovered = matches!(
        app.hovered(),
        Some(HitTarget::SeparatorDragHandle(id)) if id == layout.separator_id
    );
    let dragging = app.dragged_item() == Some(item_id);
    let surface_style = match (focused || selected, body_hovered) {
        (_, true) => Some(theme.hovered_style()),
        (true, false) => Some(theme.focused_style()),
        (false, false) => None,
    };
    if let Some(style) = surface_style {
        frame.render_widget(Block::default().style(style), layout.area);
    }
    if let Some(line) = layout.line {
        let emphasized = focused
            || selected
            || body_hovered
            || gutter_hovered
            || dragging
            || app.drag_target() == Some(layout.index);
        let style = Style::default()
            .fg(if body_hovered && (focused || selected) {
                theme.foreground
            } else if emphasized {
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
    let symbol = if focused || gutter_hovered {
        "⋮"
    } else {
        " "
    };
    let padding = usize::from(layout.gutter.height.saturating_sub(1) / 2);
    let style = if focused && dragging {
        Style::default()
            .fg(theme.on_accent)
            .bg(theme.accent_surface)
            .remove_modifier(Modifier::REVERSED | Modifier::ITALIC)
            .add_modifier(Modifier::DIM)
    } else if focused && gutter_hovered {
        theme
            .hovered_style()
            .fg(theme.accent)
            .remove_modifier(Modifier::REVERSED | Modifier::ITALIC)
    } else if focused {
        Style::default()
            .fg(theme.on_accent)
            .bg(theme.accent_surface)
            .remove_modifier(Modifier::REVERSED | Modifier::ITALIC)
            .add_modifier(Modifier::BOLD)
    } else if gutter_hovered {
        theme.hovered_style().fg(theme.accent)
    } else {
        Style::default()
            .fg(theme.accent)
            .remove_modifier(Modifier::BOLD | Modifier::ITALIC)
    };
    frame.render_widget(Block::default().style(style), layout.gutter);
    frame.render_widget(
        Paragraph::new(symbol).style(style),
        crate::ui::geometry::row(layout.gutter, u16::try_from(padding).unwrap_or(u16::MAX)),
    );
}
