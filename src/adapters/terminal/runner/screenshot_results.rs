//! Screenshot watcher results applied on the owner UI lane.

use std::{sync::mpsc::TryRecvError, time::Duration};

use crate::{
    adapters::terminal::{TerminalError, screenshot_lane::ScreenshotResult},
    ports::screenshot::ScreenshotError,
    ui::BoardApp,
};

use super::{
    CaptureRuntime, PendingWork, WorkerLanes,
    fairness::{DrainOutcome, drain_bounded},
};

pub(super) fn drain(
    app: &mut BoardApp,
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
    monotonic_now: Duration,
) -> Result<DrainOutcome, TerminalError> {
    drain_bounded(
        || match lanes.screenshot.receiver.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) if lanes.screenshot.stopped_cleanly() => Ok(None),
            Err(TryRecvError::Disconnected) => Err(lanes
                .screenshot
                .worker_failure()
                .unwrap_or(TerminalError::Worker("screenshot result lane disconnected"))),
        },
        |result| {
            let effects = apply_result(app, pending, capture, monotonic_now, result)?;
            super::durability::enqueue_effects(app, lanes, effects, pending)?;
            Ok(true)
        },
    )
}

fn apply_result(
    app: &mut BoardApp,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
    monotonic_now: Duration,
    result: ScreenshotResult,
) -> Result<Vec<crate::application::Effect>, TerminalError> {
    let effects = match result {
        ScreenshotResult::Started(lease) => {
            pending.screenshot = pending.screenshot.saturating_sub(1);
            if capture.lease.replace(lease).is_some() {
                return Err(TerminalError::Worker(
                    "screenshot lane returned overlapping capture leases",
                ));
            }
            capture.takeover_stopping = false;
            capture.watcher_stopped = false;
            capture.release_deadline = None;
            app.screenshot_started(monotonic_now);
            Vec::new()
        }
        ScreenshotResult::Candidates(candidates) => app.queue_screenshot_candidates(candidates),
        ScreenshotResult::Conflict(Some(owner)) => {
            pending.screenshot = pending.screenshot.saturating_sub(1);
            if owner.capture_protocol == crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION
                && owner.control_protocol == crate::ports::control::CONTROL_PROTOCOL_VERSION
            {
                app.screenshot_conflict(*owner);
                Vec::new()
            } else {
                app.screenshot_failed(&ScreenshotError::IncompatibleOwner)
            }
        }
        ScreenshotResult::Conflict(None) => {
            pending.screenshot = pending.screenshot.saturating_sub(1);
            app.screenshot_failed(&ScreenshotError::Ownership)
        }
        ScreenshotResult::Stopped(candidates) => {
            pending.screenshot = pending.screenshot.saturating_sub(1);
            let effects = app.queue_screenshot_candidates(candidates);
            app.screenshot_stopping_completed();
            capture.release_when_drained = true;
            capture.watcher_stopped = true;
            capture.release_deadline.get_or_insert_with(|| {
                std::time::Instant::now() + crate::ports::screenshot::CAPTURE_TEARDOWN_TIMEOUT
            });
            effects
        }
        ScreenshotResult::Failed {
            error,
            release_when_drained,
        } => {
            pending.screenshot = pending.screenshot.saturating_sub(1);
            if !release_when_drained {
                capture.lease = None;
            }
            capture.release_when_drained = release_when_drained;
            capture.watcher_stopped = release_when_drained;
            if release_when_drained {
                capture.release_deadline.get_or_insert_with(|| {
                    std::time::Instant::now() + crate::ports::screenshot::CAPTURE_TEARDOWN_TIMEOUT
                });
                app.screenshot_release_failed(&error);
                Vec::new()
            } else {
                app.screenshot_failed(&error)
            }
        }
    };
    Ok(effects)
}

pub(super) fn release_if_drained(
    app: &mut BoardApp,
    capture: &mut CaptureRuntime,
) -> Vec<crate::application::Effect> {
    let deadline_expired = capture
        .release_deadline
        .is_some_and(|deadline| std::time::Instant::now() >= deadline);
    if capture.release_when_drained
        && (!app.screenshot_blocks_capture_release() || deadline_expired)
    {
        capture.lease = None;
        capture.release_when_drained = false;
        capture.takeover_stopping = false;
        capture.release_deadline = None;
        return app.screenshot_authority_released();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::{
            editor::RopeEditorFactory, memory::FakeIdGenerator, runtime::FileRuntimeCoordinator,
        },
        application::{AppState, DurabilityState, Effect, FailureCode, ScreenshotPauseReason},
        domain::{Session, SessionBoard, Timestamp},
        ports::{
            environment::IdGenerator as _,
            runtime::{CaptureCoordinator as _, RuntimeCoordinator as _},
            screenshot::{
                ScreenshotActivityPolicy, ScreenshotCandidate, ScreenshotFingerprint,
                ScreenshotImageType,
            },
            store::StoreError,
        },
    };

    #[test]
    fn missing_conflict_metadata_is_reported_as_ownership_failure() {
        let mut ids = FakeIdGenerator::new(1_725_269_000_000);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir(),
            Timestamp::from_millis(1),
        )
        .expect("session");
        let mut app = BoardApp::new(
            AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
            RopeEditorFactory,
        );
        let mut pending = PendingWork {
            screenshot: 1,
            ..PendingWork::default()
        };
        let mut capture = CaptureRuntime::default();
        let effects = apply_result(
            &mut app,
            &mut pending,
            &mut capture,
            Duration::ZERO,
            ScreenshotResult::Conflict(None),
        )
        .expect("conflict result");
        assert!(effects.is_empty());
        assert!(
            app.status_text()
                .is_some_and(|text| text.contains("ownership is unavailable"))
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "real installation-lock regression keeps both owners and release assertions visible"
    )]
    fn final_reconcile_failure_releases_idle_capture_for_another_owner() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let runtime = temporary.path().join("runtime");
        let launch = temporary.path().join("launch");
        std::fs::create_dir(&launch).expect("launch directory");
        let mut ids = FakeIdGenerator::new(1_725_270_000_000);
        let first = FileRuntimeCoordinator::new(
            runtime.clone(),
            ids.instance_id(),
            launch.clone(),
            Timestamp::from_millis(1),
            "test",
        )
        .expect("first coordinator");
        let second = FileRuntimeCoordinator::new(
            runtime,
            ids.instance_id(),
            launch.clone(),
            Timestamp::from_millis(2),
            "test",
        )
        .expect("second coordinator");
        let mut first_session = first
            .acquire_session(ids.session_id())
            .expect("first session");
        first_session.publish_control().expect("first control");
        let mut second_session = second
            .acquire_session(ids.session_id())
            .expect("second session");
        second_session.publish_control().expect("second control");
        let lease = first
            .acquire_capture(first_session.info())
            .expect("first capture");
        let session =
            Session::new(ids.session_id(), launch, Timestamp::from_millis(1)).expect("app session");
        let mut app = BoardApp::new(
            AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
            RopeEditorFactory,
        );
        app.configure_screenshot_activity(
            ScreenshotActivityPolicy::new(1, 10).expect("activity policy"),
        );
        app.screenshot_started(Duration::ZERO);
        assert!(matches!(
            app.advance_screenshot_activity(Duration::from_secs(60))
                .as_slice(),
            [Effect::Screenshot(
                crate::application::ScreenshotIntent::Disable
            )]
        ));
        let mut pending = PendingWork {
            screenshot: 1,
            ..PendingWork::default()
        };
        let mut capture = CaptureRuntime {
            lease: Some(lease),
            ..CaptureRuntime::default()
        };

        let effects = apply_result(
            &mut app,
            &mut pending,
            &mut capture,
            Duration::ZERO,
            ScreenshotResult::Failed {
                error: ScreenshotError::Watcher,
                release_when_drained: true,
            },
        )
        .expect("apply failure");
        assert!(effects.is_empty());
        assert!(capture.lease.is_some());
        let effects = release_if_drained(&mut app, &mut capture);
        assert_eq!(
            effects,
            vec![Effect::NotifyScreenshotPause(
                ScreenshotPauseReason::Inactivity { minutes: 1 }
            )]
        );
        assert!(capture.lease.is_none());
        second
            .acquire_capture(second_session.info())
            .expect("second owner acquires released capture");
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "end-to-end failure regression keeps real lock, queued work, and retry state visible"
    )]
    fn failed_ready_capture_releases_real_lock_but_retains_explicit_retry() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let runtime = temporary.path().join("runtime");
        let launch = temporary.path().join("launch");
        std::fs::create_dir(&launch).expect("launch directory");
        let mut ids = FakeIdGenerator::new(1_725_271_000_000);
        let first = FileRuntimeCoordinator::new(
            runtime.clone(),
            ids.instance_id(),
            launch.clone(),
            Timestamp::from_millis(1),
            "test",
        )
        .expect("first coordinator");
        let second = FileRuntimeCoordinator::new(
            runtime,
            ids.instance_id(),
            launch.clone(),
            Timestamp::from_millis(2),
            "test",
        )
        .expect("second coordinator");
        let mut first_session = first
            .acquire_session(ids.session_id())
            .expect("first session");
        first_session.publish_control().expect("first control");
        let mut second_session = second
            .acquire_session(ids.session_id())
            .expect("second session");
        second_session.publish_control().expect("second control");
        let lease = first
            .acquire_capture(first_session.info())
            .expect("first capture");
        let session =
            Session::new(ids.session_id(), launch, Timestamp::from_millis(1)).expect("app session");
        let mut app = BoardApp::new(
            AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
            RopeEditorFactory,
        );
        app.screenshot_started(Duration::ZERO);
        app.queue_screenshot_candidates([
            ScreenshotCandidate {
                fingerprint: ScreenshotFingerprint([9; 32]),
                path: temporary.path().join("capture.png"),
                image_type: ScreenshotImageType::Png,
            },
            ScreenshotCandidate {
                fingerprint: ScreenshotFingerprint([10; 32]),
                path: temporary.path().join("queued.png"),
                image_type: ScreenshotImageType::Png,
            },
        ]);
        assert!(matches!(
            app.advance_screenshot_capture(
                &mut ids,
                &crate::adapters::memory::FakeClock::new(Timestamp::from_millis(3))
            )
            .as_slice(),
            [Effect::CommitCapture(_)]
        ));
        let mut capture = CaptureRuntime {
            lease: Some(lease),
            ..CaptureRuntime::default()
        };
        let _effects = release_if_drained(&mut app, &mut capture);
        assert!(
            capture.lease.is_some(),
            "in-flight commit retains authority"
        );

        let effects = app.complete_screenshot_capture(
            Err(StoreError::Busy),
            &mut ids,
            &crate::adapters::memory::FakeClock::new(Timestamp::from_millis(3)),
        );
        assert_eq!(
            effects,
            vec![Effect::Screenshot(
                crate::application::ScreenshotIntent::Disable
            )]
        );
        assert!(!capture.release_when_drained);
        let mut pending = PendingWork {
            screenshot: 1,
            ..PendingWork::default()
        };
        let effects = apply_result(
            &mut app,
            &mut pending,
            &mut capture,
            Duration::ZERO,
            ScreenshotResult::Stopped(Vec::new()),
        )
        .expect("watcher stopped");
        assert!(effects.is_empty());
        assert!(capture.release_when_drained);
        app.state.durability = DurabilityState::Failed {
            durable: crate::domain::OperationSequence::default(),
            failed: crate::domain::OperationSequence::new(1),
            code: FailureCode::StorageFailed,
        };
        let _effects = release_if_drained(&mut app, &mut capture);
        assert!(capture.lease.is_none());
        assert!(app.screenshot_retry_ready());
        second
            .acquire_capture(second_session.info())
            .expect("second owner acquires despite retained retry");
    }
}
