//! Verified screenshot-capture ownership transfer.

use std::sync::mpsc::TryRecvError;

use crate::{
    adapters::{
        control::{ControlDelivery, ControlEnvelope},
        terminal::TerminalError,
    },
    ports::{
        control::{ControlMutation, ControlRejectionCode, ControlResult},
        runtime::CaptureLease as _,
    },
};

use super::super::{CaptureRuntime, PendingWork, WorkerLanes};

pub(super) fn queue(
    instance_id: crate::domain::InstanceId,
    capture: &mut CaptureRuntime,
    envelope: ControlEnvelope,
) -> bool {
    let ControlMutation::CaptureTakeover {
        expected_owner_instance_id,
        requester_instance_id,
        capture_protocol,
    } = envelope.request.mutation
    else {
        return false;
    };
    let Some(lease) = capture.lease.as_ref() else {
        reject(
            envelope,
            ControlRejectionCode::CaptureNotOwned,
            "this process no longer owns the screenshot inbox",
        );
        return false;
    };
    if lease.owner().instance_id != expected_owner_instance_id
        || expected_owner_instance_id != instance_id
        || requester_instance_id == instance_id
        || capture_protocol != crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION
    {
        reject(
            envelope,
            ControlRejectionCode::CaptureOwnerMismatch,
            "screenshot takeover does not match the authoritative live owner",
        );
        return false;
    }
    if capture.takeover_delivery.is_some()
        || capture.takeover_stopping
        || capture.release_when_drained
    {
        reject(
            envelope,
            ControlRejectionCode::CaptureTakeoverInProgress,
            "the screenshot owner is already completing a takeover",
        );
        return false;
    }
    let result = ControlResult::Capture(
        crate::ports::control::ControlCaptureReceipt::TakeoverScheduled {
            owner_instance_id: instance_id,
        },
    );
    capture.takeover_delivery = Some(envelope.respond_confirmed(result));
    true
}

fn reject(envelope: ControlEnvelope, code: ControlRejectionCode, message: &'static str) {
    envelope.respond(ControlResult::Rejected {
        code: code.as_str().to_owned(),
        message: message.to_owned(),
    });
}

pub(super) fn complete(
    lanes: &WorkerLanes<'_>,
    pending: &mut PendingWork,
    capture: &mut CaptureRuntime,
) -> Result<bool, TerminalError> {
    let Some(delivery) = capture.takeover_delivery.take() else {
        return Ok(false);
    };
    match delivery.try_recv() {
        Ok(ControlDelivery::Delivered) => {
            lanes.screenshot.disable()?;
            pending.screenshot = pending.screenshot.saturating_add(1);
            capture.takeover_stopping = true;
            capture.release_deadline = Some(
                std::time::Instant::now() + crate::ports::screenshot::CAPTURE_TEARDOWN_TIMEOUT,
            );
            Ok(true)
        }
        Ok(ControlDelivery::Failed) | Err(TryRecvError::Disconnected) => Ok(false),
        Err(TryRecvError::Empty) => {
            capture.takeover_delivery = Some(delivery);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{
        adapters::{
            control::pending_for_test, editor::RopeEditorFactory, memory::FakeIdGenerator,
            runtime::FileRuntimeCoordinator,
        },
        application::AppState,
        domain::{Session, SessionBoard, Timestamp},
        ports::{
            control::{CONTROL_PROTOCOL_VERSION, ControlRequest},
            environment::IdGenerator as _,
            runtime::{CaptureCoordinator as _, RuntimeCoordinator as _},
        },
        ui::BoardApp,
    };

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "direct owner takeover regression keeps queue, delivery confirmation, and real lock release visible"
    )]
    fn owner_queues_confirms_and_releases_one_verified_takeover() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let runtime = temporary.path().join("runtime");
        let launch = temporary.path().join("launch");
        std::fs::create_dir(&launch).expect("launch directory");
        let mut ids = FakeIdGenerator::new(1_725_273_000_000);
        let owner = FileRuntimeCoordinator::new(
            runtime.clone(),
            ids.instance_id(),
            launch.clone(),
            Timestamp::from_millis(1),
            "test",
        )
        .expect("owner coordinator");
        let requester = FileRuntimeCoordinator::new(
            runtime,
            ids.instance_id(),
            launch.clone(),
            Timestamp::from_millis(2),
            "test",
        )
        .expect("requester coordinator");
        let mut owner_session = owner
            .acquire_session(ids.session_id())
            .expect("owner session");
        owner_session.publish_control().expect("owner control");
        let mut requester_session = requester
            .acquire_session(ids.session_id())
            .expect("requester session");
        requester_session
            .publish_control()
            .expect("requester control");
        let owner_instance = owner_session.info().instance_id;
        let requester_instance = requester_session.info().instance_id;
        let lease = owner
            .acquire_capture(owner_session.info())
            .expect("owner capture");
        let request = ControlRequest {
            protocol: CONTROL_PROTOCOL_VERSION,
            request_id: ids.request_id(),
            session_id: owner_session.info().session_id,
            mutation: ControlMutation::CaptureTakeover {
                expected_owner_instance_id: owner_instance,
                requester_instance_id: requester_instance,
                capture_protocol: crate::ports::control::CAPTURE_CONTROL_PROTOCOL_VERSION,
            },
        };
        let (envelope, response) = pending_for_test(request);
        let mut capture = CaptureRuntime {
            lease: Some(lease),
            ..CaptureRuntime::default()
        };

        assert!(queue(owner_instance, &mut capture, envelope));
        let response = response
            .recv_timeout(Duration::from_secs(1))
            .expect("scheduled response");
        assert!(response.is_confirmed());
        assert_eq!(
            response.response.result,
            ControlResult::Capture(
                crate::ports::control::ControlCaptureReceipt::TakeoverScheduled {
                    owner_instance_id: owner_instance,
                }
            )
        );
        response.complete(true);
        assert_eq!(
            capture
                .takeover_delivery
                .as_ref()
                .expect("delivery receipt")
                .try_recv(),
            Ok(ControlDelivery::Delivered)
        );
        assert!(requester.acquire_capture(requester_session.info()).is_err());

        capture.takeover_delivery = None;
        capture.release_when_drained = true;
        let session = Session::new(
            owner_session.info().session_id,
            launch,
            Timestamp::from_millis(1),
        )
        .expect("app session");
        let mut app = BoardApp::new(
            AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
            RopeEditorFactory,
        );
        let _effects =
            super::super::super::screenshot_results::release_if_drained(&mut app, &mut capture);
        requester
            .acquire_capture(requester_session.info())
            .expect("requester acquires released capture");
    }
}
