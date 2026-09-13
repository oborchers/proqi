//! Irreversible post-install schema quiescence for exact prepared participants.

use crate::{
    domain::{InstanceId, RequestId, StableVersion},
    ports::{
        runtime::InstanceInfo,
        update::{UpdateParticipantGateway, UpdateQuiesceRequest},
    },
};

pub(super) struct QuiescenceProgress {
    pub(super) requested: usize,
    pub(super) quiesced: Vec<InstanceInfo>,
    pub(super) unchanged: Vec<InstanceInfo>,
    pub(super) failed: Vec<InstanceId>,
}

pub(super) fn quiesce_prepared<G: UpdateParticipantGateway>(
    gateway: &mut G,
    prepared: &[InstanceInfo],
    operation_id: RequestId,
    installed: &StableVersion,
) -> QuiescenceProgress {
    let request = UpdateQuiesceRequest {
        operation_id,
        installed_version: installed.clone(),
    };
    let mut progress = QuiescenceProgress {
        requested: 0,
        quiesced: Vec::new(),
        unchanged: Vec::new(),
        failed: Vec::new(),
    };
    for participant in prepared {
        if participant.version == installed.to_string() {
            let _released = gateway.release(participant, operation_id);
            progress.unchanged.push(participant.clone());
            continue;
        }
        progress.requested = progress.requested.saturating_add(1);
        match gateway.quiesce(participant, &request) {
            Ok(reply)
                if reply.instance_id == participant.instance_id
                    && reply.session_id == participant.session_id =>
            {
                progress.quiesced.push(participant.clone());
            }
            Ok(_) | Err(_) => progress.failed.push(participant.instance_id),
        }
    }
    progress
}
