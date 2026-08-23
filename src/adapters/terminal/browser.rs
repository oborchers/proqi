//! Full-screen terminal runner for the session browser.

use std::{io::stdout, sync::mpsc::RecvTimeoutError, time::Duration};

use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

use crate::{
    domain::{SessionId, Timestamp},
    ui::{BrowserAction, SessionBrowser, SessionBrowserItem, Theme, UiSettings, render_browser},
};

use super::{
    TerminalError,
    control::{CrosstermControl, TerminalGuard, TerminationGuard},
    input::{InputLane, InputMessage},
    runner::supports_true_color,
};

/// Pick one resumable session and restore the terminal before returning it.
///
/// # Errors
///
/// Returns a typed setup, rendering, input, or restoration failure.
pub(crate) fn pick_session(
    items: Vec<SessionBrowserItem>,
    now: Timestamp,
    settings: &UiSettings,
) -> Result<Option<SessionId>, TerminalError> {
    let guard = TerminalGuard::enter(CrosstermControl)?;
    let termination = TerminationGuard::register()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let input = InputLane::spawn();
    let theme = Theme::resolve(settings.theme, supports_true_color());
    let mut browser = SessionBrowser::new(items, now);
    let run_result = drive(&mut terminal, &mut browser, &input, &termination, &theme);
    let input_result = input
        .stop()
        .map_err(|_| TerminalError::Worker("browser input lane panicked"));
    drop(terminal);
    let restoration_result = guard.finish();
    let selected = run_result?;
    input_result?;
    restoration_result?;
    Ok(selected)
}

fn drive(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    browser: &mut SessionBrowser,
    input: &InputLane,
    termination: &TerminationGuard,
    theme: &Theme,
) -> Result<Option<SessionId>, TerminalError> {
    let mut dirty = true;
    loop {
        if termination.requested() {
            return Ok(None);
        }
        if dirty {
            terminal.draw(|frame| {
                let layout = browser.prepare_frame(frame.area());
                render_browser(frame, browser, &layout, theme);
            })?;
            dirty = false;
        }
        match input.receiver.recv_timeout(Duration::from_millis(40)) {
            Ok(InputMessage::Event(event)) => match browser.handle(event) {
                BrowserAction::Continue => dirty = true,
                BrowserAction::Open(id) => return Ok(Some(id)),
                BrowserAction::Cancel => return Ok(None),
            },
            Ok(InputMessage::Failed(message)) => return Err(TerminalError::Io(message)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(TerminalError::Worker("browser input lane disconnected"));
            }
        }
    }
}
