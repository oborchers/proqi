//! In-flight work tracked by the owner reducer.

use std::collections::BTreeMap;
use std::collections::VecDeque;

use crate::{
    adapters::control::{ControlDeliveryReceipt, ControlEnvelope},
    domain::{OperationSequence, RequestId, ThoughtId},
};

#[derive(Default)]
pub(super) struct PendingWork {
    pub(super) persistence: usize,
    pub(super) external: usize,
    pub(super) controls: BTreeMap<OperationSequence, PendingControl>,
    pub(super) control_lookups: BTreeMap<RequestId, ControlEnvelope>,
    pub(super) update_prepares: BTreeMap<RequestId, ControlEnvelope>,
    pub(super) metadata_controls: BTreeMap<RequestId, ControlEnvelope>,
    pub(super) sync_controls: VecDeque<ControlEnvelope>,
    pub(super) update_restart: Option<PendingUpdateRestart>,
    pub(super) update: usize,
}

impl PendingWork {
    pub(super) fn is_empty(&self) -> bool {
        self.persistence == 0
            && self.external == 0
            && self.controls.is_empty()
            && self.control_lookups.is_empty()
            && self.update_prepares.is_empty()
            && self.metadata_controls.is_empty()
            && self.sync_controls.is_empty()
            && self.update_restart.is_none()
            && self.update == 0
    }
}

pub(super) struct PendingControl {
    pub(super) envelope: ControlEnvelope,
    pub(super) thought_id: Option<ThoughtId>,
}

pub(super) struct PendingUpdateRestart {
    pub(super) operation_id: RequestId,
    pub(super) delivery: ControlDeliveryReceipt,
}
