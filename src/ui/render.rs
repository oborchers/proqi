//! Deterministic one-column board renderer.

mod chrome;
mod content;
mod global_delivery;
mod overlay_composition;
mod overlays;
mod release_highlights;

use crate::{application::InteractionMode, ports::text_layout::wrap_rows};
use ratatui_core::{
    layout::Alignment,
    style::{Modifier, Style},
    terminal::Frame,
    text::{Line, Span, Text},
};
use ratatui_widgets::{block::Block, clear::Clear, paragraph::Paragraph};

use super::{
    BoardApp, HitTarget, LayoutSnapshot, Theme, ThoughtLayout, app::InvocationChoiceView,
    layout::OverlayLayout,
};
use content::{styled_line, url_ranges};

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
            selection: app.overlay_query_selection(),
            entries: &rows,
            selected,
            hovered: app.hovered(),
        },
        app.picker_overflow(overlay.items.len()),
        theme,
    );
}

pub(super) fn render_command_picker(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    app: &BoardApp,
    picker: &crate::ui::app::CommandPaletteView,
    theme: &Theme,
) {
    let rows = picker
        .rows
        .iter()
        .map(|row| {
            overlays::PickerRow::command(
                &row.primary,
                row.secondary.as_deref(),
                &row.secondary_fallbacks,
                &row.protected_secondaries,
                row.group,
                row.enabled,
            )
        })
        .collect::<Vec<_>>();
    overlays::render_picker(
        frame,
        overlay,
        overlays::PickerView {
            title: " commands ",
            prompt: ':',
            query: &picker.query,
            cursor: app.overlay_query_cursor().unwrap_or(picker.query.len()),
            selection: app.overlay_query_selection(),
            entries: &rows,
            selected: picker.selected,
            hovered: app.hovered(),
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
            selection: app.overlay_query_selection(),
            entries: &rows,
            selected,
            hovered: app.hovered(),
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
        render_board_thought(frame, app, &presentation, editor, thought_layout, theme);
    }
    if let Some(compose) = &layout.compose {
        frame.render_widget(Block::default().style(theme.focused_style()), compose.area);
        render_compose_gutter(frame, compose, theme);
        render_editor(frame, app, editor, compose.text_area, None, theme);
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
        let style = if !prompt && hovered {
            theme.hovered_style()
        } else if !prompt && app.insertion_focused() {
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

fn render_board_thought(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    presentation: &crate::ui::projection::FramePresentation,
    editor: Option<&crate::ui::projection::EditorPresentation>,
    layout: &ThoughtLayout,
    theme: &Theme,
) {
    let Some(thought) = presentation.thought(layout.thought_id) else {
        return;
    };
    let focused = app.active_thought_id() == Some(layout.thought_id);
    let selected = app.thought_selected(layout.thought_id);
    let hovered = matches!(app.hovered(), Some(HitTarget::Thought(id)) if id == layout.thought_id);
    let hovered_fold = match app.hovered() {
        Some(HitTarget::Fold(id, index)) if id == layout.thought_id => Some(index),
        _ => None,
    };
    let gutter_hovered =
        matches!(app.hovered(), Some(HitTarget::DragHandle(id)) if id == layout.thought_id);
    let overflow_hovered =
        matches!(app.hovered(), Some(HitTarget::Overflow(id)) if id == layout.thought_id);
    render_separator(
        frame,
        layout,
        app.drag_target() == Some(layout.index),
        theme,
    );
    let surface_style = match (focused || selected, hovered) {
        (true, true) => Some(theme.focused_hovered_style()),
        (true, false) => Some(theme.focused_style()),
        (false, true) => Some(theme.hovered_style()),
        (false, false) => None,
    };
    if let Some(style) = surface_style {
        frame.render_widget(Block::default().style(style), layout.area);
    }
    render_gutter(
        frame,
        layout,
        focused,
        gutter_hovered,
        app.dragged_thought() == Some(layout.thought_id),
        theme,
    );
    if matches!(app.interaction_mode(), InteractionMode::Edit { thought_id } if thought_id == layout.thought_id)
    {
        render_editor(frame, app, editor, layout.text_area, hovered_fold, theme);
    } else {
        render_thought(
            frame,
            app,
            &thought.presentation,
            layout,
            ThoughtEmphasis {
                surface: focused || selected,
                overflow_hovered,
                hovered_fold,
            },
            theme,
        );
    }
}

#[derive(Clone, Copy)]
struct ThoughtEmphasis {
    surface: bool,
    overflow_hovered: bool,
    hovered_fold: Option<usize>,
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
    let surface_style = if focused {
        let modifier = if dragging {
            Modifier::DIM
        } else if hovered {
            Modifier::BOLD | Modifier::ITALIC
        } else {
            Modifier::BOLD
        };
        Style::default()
            .fg(theme.on_accent)
            .bg(theme.accent_surface)
            .remove_modifier(Modifier::REVERSED | Modifier::ITALIC)
            .add_modifier(modifier)
    } else if hovered {
        theme.hovered_style().fg(theme.accent)
    } else {
        Style::default()
            .fg(theme.accent)
            .remove_modifier(Modifier::BOLD | Modifier::ITALIC)
    };
    frame.render_widget(Block::default().style(surface_style), layout.gutter);
    frame.render_widget(
        Paragraph::new(Span::styled(symbol, surface_style)),
        crate::ui::geometry::row(layout.gutter, u16::try_from(padding).unwrap_or(u16::MAX)),
    );
}

fn render_thought(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    presentation: &crate::ui::annotations::Presentation,
    layout: &ThoughtLayout,
    emphasis: ThoughtEmphasis,
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
            presentation
                .substitutions
                .iter()
                .find(|fold| fold.annotation_index == emphasis.hovered_fold.unwrap_or(usize::MAX)),
            theme,
        )
    })
    .collect::<Vec<_>>();
    let mut paragraph = Paragraph::new(Text::from(rendered_lines));
    if emphasis.surface {
        paragraph = paragraph.style(Style::default().fg(theme.foreground));
    } else if layout.hidden_rows > 0 {
        paragraph = paragraph.style(Style::default().fg(theme.muted));
    }
    frame.render_widget(paragraph, layout.text_area);
    if let Some(overflow) = layout.overflow {
        frame.render_widget(Clear, overflow);
        frame.render_widget(
            Paragraph::new(format!("{} more lines  expand", layout.hidden_rows)).style(
                if emphasis.overflow_hovered {
                    theme.hovered_style().fg(theme.accent)
                } else {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::DIM)
                },
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
    hovered_fold: Option<usize>,
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
                presentation
                    .substitutions
                    .iter()
                    .find(|fold| fold.annotation_index == hovered_fold.unwrap_or(usize::MAX)),
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

#[cfg(test)]
#[path = "render/tests.rs"]
mod tests;
