use ratatui_core::{backend::TestBackend, terminal::Terminal};

use crate::ui::{Theme, ThemePreference, render};

use super::*;

pub(super) fn live_snapshot(width: u16, height: u16) -> String {
    let cwd = tempfile::tempdir().expect("tempdir");
    let (mut app, _, _) = app("Coordinate", cwd.path());
    install_catalog(&mut app, cwd.path(), Vec::new());
    open_with_live(
        &mut app,
        vec![
            reviewer(AgentState::Working),
            reference(
                "builder",
                ("w3", Some("Implementation")),
                ("w3:t2", Some("Build")),
                "w3:p7",
                AgentState::Idle,
            ),
        ],
    );
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            let layout = app.prepare_frame(frame.area());
            render(
                frame,
                &app,
                &layout,
                &Theme::resolve(ThemePreference::Dark, true),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|row| {
            let content = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect::<String>();
            format!("{row:02}│{}│", content.trim_end_matches(' '))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
