use std::{fs, os::unix::fs::PermissionsExt as _, sync::Arc, time::Duration};

use super::{ScreenshotLane, ScreenshotResult};
use crate::{
    adapters::{
        memory::FakeIdGenerator, runtime::FileRuntimeCoordinator, terminal::settings::load_settings,
    },
    domain::Timestamp,
    ports::{
        environment::IdGenerator as _,
        runtime::RuntimeCoordinator as _,
        screenshot::{
            ActiveScreenshotWatcher, ScreenshotCancellation, ScreenshotCandidate, ScreenshotError,
            ScreenshotInboxConfig, ScreenshotWatcherFactory,
        },
    },
};

struct FailingFinalFactory;

impl ScreenshotWatcherFactory for FailingFinalFactory {
    fn start(
        &self,
        _config: ScreenshotInboxConfig,
        _terminal_host: &str,
        _cancellation: Arc<dyn ScreenshotCancellation>,
    ) -> Result<Box<dyn ActiveScreenshotWatcher>, ScreenshotError> {
        Ok(Box::new(FailingFinalWatcher))
    }
}

struct FailingFinalWatcher;

impl ActiveScreenshotWatcher for FailingFinalWatcher {
    fn poll(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        Ok(Vec::new())
    }

    fn final_reconcile(&mut self) -> Result<Vec<ScreenshotCandidate>, ScreenshotError> {
        Err(ScreenshotError::Reconciliation)
    }
}

#[test]
fn disable_reports_that_final_reconcile_failure_must_release_after_drain() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let runtime = temporary.path().join("runtime");
    let watched = temporary.path().join("watched");
    let config_directory = temporary.path().join("config");
    fs::create_dir(&watched).expect("watched directory");
    fs::create_dir(&config_directory).expect("config directory");
    let config_path = config_directory.join("config.toml");
    fs::write(
        &config_path,
        format!("[screenshot_inbox]\ndirectory = '{}'\n", watched.display()),
    )
    .expect("config");
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).expect("private config");
    let settings = load_settings(&config_directory)
        .expect("settings")
        .screenshot;
    let mut ids = FakeIdGenerator::new(1_725_271_000_000);
    let coordinator = FileRuntimeCoordinator::new(
        runtime,
        ids.instance_id(),
        temporary.path().to_path_buf(),
        Timestamp::from_millis(1),
        "test",
    )
    .expect("coordinator");
    let mut session = coordinator
        .acquire_session(ids.session_id())
        .expect("session");
    session.publish_control().expect("control");
    let lane = ScreenshotLane::spawn_with_factory(
        coordinator,
        session.info().clone(),
        settings,
        "Test Terminal".to_owned(),
        Arc::new(FailingFinalFactory),
    );

    lane.enable().expect("enable");
    let started = lane
        .receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("started");
    let ScreenshotResult::Started(_lease) = started else {
        panic!("capture started");
    };
    lane.disable().expect("disable");
    let failed = lane
        .receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("failed final reconcile");
    assert!(matches!(
        failed,
        ScreenshotResult::Failed {
            error: ScreenshotError::Reconciliation,
            release_when_drained: true,
        }
    ));
    lane.stop(super::super::supervisor::ShutdownDeadline::after(
        Duration::from_secs(1),
    ))
    .expect("bounded stop");
}
