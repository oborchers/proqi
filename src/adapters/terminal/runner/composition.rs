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
    mouse_capture: bool,
) -> Result<(crate::ui::Theme, TerminalGuard<CrosstermControl>), TerminalError> {
    let theme = super::super::palette::resolve(recipe, super::supports_true_color())?;
    let guard = TerminalGuard::enter(CrosstermControl::new(keyboard, mouse_capture))?;
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
    state_root: Option<&std::path::Path>,
) -> OwnedLanes {
    let cancellation = crate::adapters::process::CancellationFlag::default();
    OwnedLanes {
        accessibility: AccessibilityLane::spawn(executable, cancellation.clone()),
        control,
        input: InputLane::spawn_with_test_acceptance(state_root),
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

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        adapters::{
            runtime::{FileRuntimeCoordinator, SystemIdGenerator},
            update::FileUpdateStateStore,
        },
        domain::{InstallationIdentity, Timestamp},
        ports::{
            environment::IdGenerator as _,
            runtime::RuntimeCoordinator as _,
            update::{UpdateLockKind, UpdateStateStore as _},
        },
    };

    use super::{publish_optional_control, start_optional_control};

    #[test]
    fn failed_control_publication_retains_convergence_admission_for_owner_lifetime() {
        let temporary = tempfile::tempdir().expect("temporary state");
        let runtime = temporary.path().join("runtime");
        let cache = temporary.path().join("cache");
        let installation = InstallationIdentity::from_digest([73; 32]);
        let update_state = FileUpdateStateStore::new(&cache).expect("update state");
        let admission = update_state
            .try_startup_lock(installation)
            .expect("startup admission")
            .expect("startup lease");
        let mut ids = SystemIdGenerator;
        let coordinator = FileRuntimeCoordinator::new(
            runtime.clone(),
            ids.instance_id(),
            temporary.path().to_path_buf(),
            Timestamp::from_millis(1),
            "0.9.0",
        )
        .expect("runtime coordinator");
        let mut session = coordinator
            .acquire_session(ids.session_id())
            .expect("session lease");
        let (mut control, mut warning) = start_optional_control(&session);
        assert!(
            control.is_some(),
            "control must bind before publication fails: endpoint={:?}, warning={warning:?}",
            session.control_endpoint()
        );

        let metadata = runtime
            .join("instances")
            .join(format!("{}.json", session.info().instance_id));
        fs::remove_file(metadata).expect("remove initial metadata");
        fs::remove_dir(runtime.join("instances")).expect("remove metadata directory");
        fs::write(runtime.join("instances"), b"publication blocked")
            .expect("block metadata publication");

        let control_ready = publish_optional_control(&mut session, &mut control, &mut warning);
        assert!(!control_ready);
        assert!(control.is_none());
        assert!(warning.is_some());
        let retained = super::super::admission::retain_unpublished_startup_admission(
            Some(admission),
            control_ready,
        );
        assert!(
            update_state
                .try_lock(installation, UpdateLockKind::Convergence)
                .expect("contended convergence")
                .is_none(),
            "an unpublished live owner must exclude installation"
        );
        drop((session, retained));
        assert!(
            update_state
                .try_lock(installation, UpdateLockKind::Convergence)
                .expect("released convergence")
                .is_some()
        );
    }
}
