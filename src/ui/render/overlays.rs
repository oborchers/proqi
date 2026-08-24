//! Compact contextual help and searchable modal pickers.

use ratatui_core::{
    style::{Modifier, Style},
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_widgets::{
    block::Block,
    borders::Borders,
    clear::Clear,
    paragraph::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr as _;

use crate::application::InteractionMode;

use super::super::{BoardApp, Theme, layout::OverlayLayout};

pub(super) fn render_help(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    overlay: &OverlayLayout,
    theme: &Theme,
) {
    frame.render_widget(Clear, overlay.area);
    frame.render_widget(
        Paragraph::new(help_lines(app, theme, overlay.area.width.saturating_sub(2)))
            .block(
                Block::default()
                    .title(Span::styled(
                        " proqi shortcuts ",
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    ))
                    .borders(Borders::ALL),
            )
            .wrap(Wrap { trim: true }),
        overlay.area,
    );
    render_close(frame, overlay, theme);
}

pub(super) fn render_picker(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    picker: PickerView<'_>,
    theme: &Theme,
) {
    frame.render_widget(Clear, overlay.area);
    frame.render_widget(
        Block::default().title(picker.title).borders(Borders::ALL),
        overlay.area,
    );
    let input = input_area(overlay);
    frame.render_widget(
        Paragraph::new(format!("{}{query}", picker.prompt, query = picker.query))
            .style(theme.focused_style()),
        input,
    );
    for (index, (entry, area)) in picker.entries.iter().zip(&overlay.items).enumerate() {
        let style = if index == picker.selected {
            theme
                .focused_style()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.base_style()
        };
        frame.render_widget(Paragraph::new(entry.as_str()).style(style), *area);
    }
    render_close(frame, overlay, theme);
}

pub(super) fn render_text_prompt(
    frame: &mut Frame<'_>,
    overlay: &OverlayLayout,
    title: &str,
    value: &str,
    theme: &Theme,
) {
    frame.render_widget(Clear, overlay.area);
    frame.render_widget(
        Block::default().title(title).borders(Borders::ALL),
        overlay.area,
    );
    frame.render_widget(
        Paragraph::new(format!("> {value}")).style(theme.focused_style()),
        input_area(overlay),
    );
    render_close(frame, overlay, theme);
    let x = overlay
        .area
        .x
        .saturating_add(3)
        .saturating_add(u16::try_from(value.width()).unwrap_or(u16::MAX))
        .min(overlay.area.right().saturating_sub(2));
    frame.set_cursor_position((x, overlay.area.y.saturating_add(1)));
}

fn input_area(overlay: &OverlayLayout) -> ratatui_core::layout::Rect {
    ratatui_core::layout::Rect::new(
        overlay.area.x.saturating_add(1),
        overlay.area.y.saturating_add(1),
        overlay.area.width.saturating_sub(2),
        u16::from(overlay.area.height > 2),
    )
}

#[derive(Clone, Copy)]
pub(super) struct PickerView<'a> {
    pub(super) title: &'a str,
    pub(super) prompt: char,
    pub(super) query: &'a str,
    pub(super) entries: &'a [String],
    pub(super) selected: usize,
}

fn render_close(frame: &mut Frame<'_>, overlay: &OverlayLayout, theme: &Theme) {
    frame.render_widget(
        Paragraph::new("[x]").style(Style::default().fg(theme.accent)),
        overlay.close,
    );
}

fn help_lines(app: &BoardApp, theme: &Theme, width: u16) -> Vec<Line<'static>> {
    let keys = app.keybindings();
    let items = if matches!(app.interaction_mode(), InteractionMode::Edit { .. }) {
        edit_shortcuts(keys)
    } else {
        board_shortcuts(app)
    };
    shortcut_grid(&items, width, theme)
}

fn edit_shortcuts(keys: &crate::ui::KeyBindings) -> Vec<(String, &'static str)> {
    vec![
        ("Esc".to_owned(), "Board"),
        (primary("A"), "Select all"),
        (primary("U"), "Delete line"),
        (primary("Z"), "Undo"),
        (primary("Shift+Z"), "Redo"),
        (keys.commands.to_string(), "Commands"),
        (keys.help.to_string(), "Close"),
    ]
}

fn board_shortcuts(app: &BoardApp) -> Vec<(String, &'static str)> {
    let keys = app.keybindings();
    let mut delivery = Vec::new();
    if app.supports_submission() {
        delivery.push((keys.submit_remove.to_string(), "Submit & remove"));
        delivery.push((keys.submit_keep.to_string(), "Submit & keep"));
    }
    let mut items = vec![
        (keys.new.to_string(), "New"),
        (format!("Enter/{}", keys.edit), "Edit"),
        (format!("{}/{}", keys.focus_down, keys.focus_up), "Move"),
        (format!("{}/{}", keys.move_down, keys.move_up), "Reorder"),
        (keys.copy.to_string(), "Copy"),
        (keys.cut.to_string(), "Cut"),
        (keys.delete.to_string(), "Delete"),
        (keys.undo.to_string(), "Undo"),
        (crate::ui::settings::key_label(keys.collapse), "Fold"),
        (keys.search.to_string(), "Search"),
        (keys.commands.to_string(), "Commands"),
        (keys.help.to_string(), "Close"),
    ];
    items.extend(delivery);
    items.push((keys.quit.to_string(), "Quit"));
    items
}

fn shortcut_grid(
    items: &[(String, &'static str)],
    width: u16,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let columns = if width >= 48 {
        3
    } else if width >= 30 {
        2
    } else {
        1
    };
    let cell_width = usize::from(width) / columns;
    let key_width = items.iter().map(|(key, _)| key.width()).max().unwrap_or(1);
    items
        .chunks(columns)
        .map(|row| shortcut_row(row, columns, cell_width, key_width, theme))
        .collect()
}

fn shortcut_row(
    items: &[(String, &'static str)],
    columns: usize,
    cell_width: usize,
    key_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (key, label)) in items.iter().enumerate() {
        spans.push(Span::styled(key.clone(), Style::default().fg(theme.accent)));
        spans.push(Span::raw(
            " ".repeat(key_width.saturating_sub(key.width()) + 1),
        ));
        spans.push(Span::styled(*label, Style::default().fg(theme.foreground)));
        if index + 1 < columns {
            let used = key_width + 1 + label.width();
            spans.push(Span::raw(" ".repeat(cell_width.saturating_sub(used))));
        }
    }
    Line::from(spans)
}

fn primary(suffix: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("⌘{suffix}")
    } else {
        format!("Ctrl+{suffix}")
    }
}
