//! Owned worker-lane composition.

use std::path::PathBuf;

use crate::{
    adapters::{
        control::ControlServer,
        runtime::FileRuntimeCoordinator,
        sqlite::SqliteStore,
        terminal::{
            external::ExternalLane, input::InputLane, persistence::PersistenceLane,
            screenshot_lane::ScreenshotLane,
        },
    },
    domain::InstanceId,
    ports::runtime::InstanceInfo,
};

use super::owned_lanes::OwnedLanes;

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
) -> OwnedLanes {
    let cancellation = crate::adapters::process::CancellationFlag::default();
    OwnedLanes {
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
            terminal_host_label(),
        ),
        cancellation,
    }
}

fn terminal_host_label() -> String {
    let value = std::env::var("TERM_PROGRAM")
        .ok()
        .filter(|value| {
            !value.is_empty() && value.chars().count() <= 80 && !value.chars().any(char::is_control)
        })
        .unwrap_or_else(|| "the terminal host running Proqi".to_owned());
    match value.as_str() {
        "Apple_Terminal" => "Terminal".to_owned(),
        "iTerm.app" => "iTerm2".to_owned(),
        "ghostty" | "Ghostty" => "Ghostty".to_owned(),
        "vscode" => "Visual Studio Code".to_owned(),
        _ => value,
    }
}
