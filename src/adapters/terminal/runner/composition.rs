//! Owned worker-lane composition.

use std::io::{IsTerminal as _, stdin, stdout};
use std::path::PathBuf;

use crate::{
    adapters::{
        control::ControlServer,
        runtime::{FileRuntimeCoordinator, FileSessionLease},
        sqlite::SqliteStore,
        terminal::{
            TerminalError,
            accessibility_lane::AccessibilityLane,
            control::{CrosstermControl, TerminalGuard},
            external::ExternalLane,
            input::InputLane,
            persistence::PersistenceLane,
            screenshot_lane::ScreenshotLane,
        },
    },
    domain::InstanceId,
    ports::runtime::InstanceInfo,
};

use super::owned_lanes::OwnedLanes;

pub(crate) fn require_interactive() -> Result<(), TerminalError> {
    if stdin().is_terminal() && stdout().is_terminal() {
        Ok(())
    } else {
        Err(TerminalError::Io(
            "interactive launch requires a terminal; use --json for scriptable output".to_owned(),
        ))
    }
}

pub(super) fn start_optional_control(
    session_lease: &FileSessionLease,
) -> (Option<ControlServer>, Option<String>) {
    let Some(endpoint) = session_lease.control_endpoint() else {
        return (
            None,
            Some("active-session CLI forwarding is unavailable on this platform".to_owned()),
        );
    };
    let server = match ControlServer::spawn(endpoint) {
        Ok(server) => server,
        Err(error) => {
            return (
                None,
                Some(format!(
                    "active-session CLI forwarding unavailable: {error}"
                )),
            );
        }
    };
    (Some(server), None)
}

pub(super) fn publish_optional_control(
    session_lease: &mut FileSessionLease,
    control: &mut Option<ControlServer>,
    warning: &mut Option<String>,
) -> bool {
    if control.is_none() {
        return false;
    }
    if let Err(error) = session_lease.publish_control() {
        if let Some(server) = control.take() {
            let _stopped = server.stop();
        }
        *warning = Some(format!(
            "active-session CLI forwarding unavailable: {error}"
        ));
        return false;
    }
    true
}

pub(super) fn enter_terminal(
    recipe: &crate::ui::ThemeRecipe,
    keyboard: crate::ui::KeyboardEnhancement,
) -> Result<(crate::ui::Theme, TerminalGuard<CrosstermControl>), TerminalError> {
    let theme = super::super::palette::resolve(recipe, super::supports_true_color())?;
    let guard = TerminalGuard::enter(CrosstermControl::new(keyboard))?;
    Ok((theme, guard))
}

#[expect(
    clippy::too_many_arguments,
    reason = "composition root owns explicit adapter inputs"
)]
pub(super) fn spawn_lanes(
    control: Option<ControlServer>,
    store: SqliteStore,
    coordinator: FileRuntimeCoordinator,
    cwd: PathBuf,
    recovery_directory: PathBuf,
    attachment_directory: PathBuf,
    presentation_source: String,
    cache_directory: PathBuf,
    installation: Option<crate::domain::Installation>,
    initiating_instance: InstanceId,
    invocation_roots: Vec<crate::ports::invocation::AdditionalInvocationRoot>,
    screenshot_settings: super::super::settings::ScreenshotSettings,
    instance: InstanceInfo,
    terminal_host: String,
    executable: PathBuf,
) -> OwnedLanes {
    let cancellation = crate::adapters::process::CancellationFlag::default();
    OwnedLanes {
        accessibility: AccessibilityLane::spawn(executable, cancellation.clone()),
        control,
        input: InputLane::spawn(),
        persistence: PersistenceLane::spawn_with_runtime(
            store,
            coordinator.clone(),
            cwd,
            cancellation.clone(),
        ),
        external: ExternalLane::spawn_with_invocation_roots(
            recovery_directory,
            attachment_directory,
            cache_directory.clone(),
            presentation_source,
            cancellation.clone(),
            invocation_roots,
        ),
        update: super::super::update_lane::UpdateLane::spawn(
            cache_directory,
            installation,
            coordinator.clone(),
            initiating_instance,
            cancellation.clone(),
        ),
        screenshot: ScreenshotLane::spawn(
            coordinator,
            instance,
            screenshot_settings,
            terminal_host,
        ),
        cancellation,
    }
}
