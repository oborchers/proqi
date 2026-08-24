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

use crate::{application::InteractionMode, ports::agent::AgentDeliveryMode};

use super::super::{BoardApp, Theme, layout::OverlayLayout};

pub(super) fn render_help(
    frame: &mut Frame<'_>,
    app: &BoardApp,
    overlay: &OverlayLayout,
    theme: &Theme,
) {
    frame.render_widget(Clear, overlay.area);
    frame.render_widget(
        Paragraph::new(help_lines(app, theme))
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
        Paragraph::new(format!("{}{query}", picker.prompt, query = picker.query))
            .block(Block::default().title(picker.title).borders(Borders::ALL)),
        overlay.area,
    );
    for (index, (entry, area)) in picker.entries.iter().zip(&overlay.items).enumerate() {
        let style = if index == picker.selected {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        frame.render_widget(Paragraph::new(entry.as_str()).style(style), *area);
    }
    render_close(frame, overlay, theme);
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

fn help_lines(app: &BoardApp, theme: &Theme) -> Vec<Line<'static>> {
    let keys = app.keybindings();
    if matches!(app.interaction_mode(), InteractionMode::Edit { .. }) {
        return vec![
            shortcut_line(&[("Esc".to_owned(), "Board")], theme),
            shortcut_line(&[(primary("A"), "Select all")], theme),
            shortcut_line(&[(primary("U"), "Delete line")], theme),
            shortcut_line(
                &[(primary("Z"), "Undo"), (primary("Shift+Z"), "Redo")],
                theme,
            ),
            shortcut_line(&[(keys.commands.to_string(), "Commands")], theme),
            shortcut_line(&[(keys.help.to_string(), "Close")], theme),
        ];
    }
    let mut delivery = Vec::new();
    if app.supports_delivery(AgentDeliveryMode::Compose) {
        delivery.push((keys.send.to_string(), "Send"));
    }
    if app.supports_delivery(AgentDeliveryMode::Submit) {
        delivery.push((keys.submit.to_string(), "Submit"));
    }
    delivery.push((keys.quit.to_string(), "Quit"));
    vec![
        shortcut_line(
            &[
                (keys.new.to_string(), "New"),
                (format!("Enter/{}", keys.edit), "Edit"),
            ],
            theme,
        ),
        shortcut_line(
            &[
                (format!("{}/{}", keys.focus_down, keys.focus_up), "Move"),
                (format!("{}/{}", keys.move_down, keys.move_up), "Reorder"),
            ],
            theme,
        ),
        shortcut_line(
            &[
                (keys.copy.to_string(), "Copy"),
                (keys.cut.to_string(), "Cut"),
                (keys.delete.to_string(), "Delete"),
            ],
            theme,
        ),
        shortcut_line(
            &[
                (keys.undo.to_string(), "Undo"),
                (crate::ui::settings::key_label(keys.collapse), "Fold"),
            ],
            theme,
        ),
        shortcut_line(
            &[
                (keys.search.to_string(), "Search"),
                (keys.commands.to_string(), "Commands"),
                (keys.help.to_string(), "Close"),
            ],
            theme,
        ),
        shortcut_line(&delivery, theme),
    ]
}

fn shortcut_line(items: &[(String, &str)], theme: &Theme) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (key, label)) in items.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(key.clone(), Style::default().fg(theme.accent)));
        spans.push(Span::styled(
            format!(" {label}"),
            Style::default().fg(theme.foreground),
        ));
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
