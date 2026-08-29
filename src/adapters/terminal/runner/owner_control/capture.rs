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
    lanes: &WorkerLanes<'_>,
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
        || expected_owner_instance_id != lanes.instance.instance_id
        || requester_instance_id == lanes.instance.instance_id
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
            owner_instance_id: lanes.instance.instance_id,
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
            Ok(true)
        }
        Ok(ControlDelivery::Failed) | Err(TryRecvError::Disconnected) => Ok(false),
        Err(TryRecvError::Empty) => {
            capture.takeover_delivery = Some(delivery);
            Ok(false)
        }
    }
}
