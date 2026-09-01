//! Peer-first update restart sequencing.

use crate::{
    domain::{
        InstallationIdentity, InstanceId, ReleaseHighlightAnnouncement, RequestId, SessionId,
        StableVersion,
    },
    ports::{
        runtime::InstanceInfo,
        update::{
            UpdateParticipantGateway, UpdateReplacementExpectation, UpdateRestartRequest,
            UpdateStateStore,
        },
    },
};

pub(super) struct RestartProgress {
    pub(super) initiating: Option<InstanceInfo>,
    pub(super) requested: usize,
    pub(super) failed: Vec<InstanceId>,
    pub(super) replacements: Vec<UpdateReplacementExpectation>,
}

pub(super) fn restart_peers<G: UpdateParticipantGateway>(
    gateway: &mut G,
    mut participants: Vec<InstanceInfo>,
    prepared: &[InstanceInfo],
    operation_id: RequestId,
    initiating_instance: InstanceId,
    installed: &StableVersion,
) -> RestartProgress {
    let restart = UpdateRestartRequest {
        operation_id,
        installed_version: installed.clone(),
    };
    let mut requested = 0_usize;
    let initiating = participants
        .iter()
        .position(|participant| participant.instance_id == initiating_instance)
        .map(|index| participants.remove(index));
    let mut failed = Vec::new();
    let mut replacements = Vec::new();
    for participant in &participants {
        if participant.version == installed.to_string() {
            release_if_prepared(gateway, participant, prepared, operation_id);
            continue;
        }
        requested = requested.saturating_add(1);
        if restart_accepted(gateway, participant, &restart) {
            replacements.push(UpdateReplacementExpectation {
                session_id: participant.session_id,
                previous_instance_id: participant.instance_id,
            });
        } else {
            failed.push(participant.instance_id);
        }
    }
    RestartProgress {
        initiating,
        requested,
        failed,
        replacements,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "initiating restart state remains explicit"
)]
pub(super) fn restart_initiating<G: UpdateParticipantGateway>(
    gateway: &mut G,
    participant: Option<&InstanceInfo>,
    prepared: &[InstanceInfo],
    operation_id: RequestId,
    initiating_instance: InstanceId,
    installed: &StableVersion,
    pending_recorded: Option<bool>,
    requested: &mut usize,
    failed: &mut Vec<InstanceId>,
) {
    let Some(participant) = participant else {
        *requested = requested.saturating_add(1);
        failed.push(initiating_instance);
        return;
    };
    if participant.version == installed.to_string() {
        release_if_prepared(gateway, participant, prepared, operation_id);
        return;
    }
    if pending_recorded == Some(false) {
        let _released = gateway.release(participant, operation_id);
        failed.push(participant.instance_id);
        return;
    }
    *requested = (*requested).saturating_add(1);
    let restart = UpdateRestartRequest {
        operation_id,
        installed_version: installed.clone(),
    };
    if !restart_accepted(gateway, participant, &restart) {
        failed.push(participant.instance_id);
    }
}

pub(super) fn initiating_upgrade(
    prepared: &[InstanceInfo],
    initiating: InstanceId,
) -> Option<(SessionId, StableVersion)> {
    prepared
        .iter()
        .find(|participant| participant.instance_id == initiating)
        .and_then(|participant| {
            StableVersion::parse(&participant.version)
                .ok()
                .map(|version| (participant.session_id, version))
        })
}

pub(super) fn record_pending_highlights<S: UpdateStateStore>(
    state: &S,
    installation: InstallationIdentity,
    session_id: Option<SessionId>,
    previous: &StableVersion,
    target: &StableVersion,
) -> Option<bool> {
    let session_id = session_id?;
    if previous == target {
        return Some(true);
    }
    let Ok(announcement) =
        ReleaseHighlightAnnouncement::pending(session_id, previous.clone(), target.clone())
    else {
        return Some(false);
    };
    Some(
        state
            .record_release_highlights(installation, announcement)
            .is_ok(),
    )
}

fn release_if_prepared<G: UpdateParticipantGateway>(
    gateway: &mut G,
    participant: &InstanceInfo,
    prepared: &[InstanceInfo],
    operation_id: RequestId,
) {
    if prepared
        .iter()
        .any(|ready| ready.instance_id == participant.instance_id)
    {
        let _released = gateway.release(participant, operation_id);
    }
}

fn restart_accepted<G: UpdateParticipantGateway>(
    gateway: &mut G,
    participant: &InstanceInfo,
    request: &UpdateRestartRequest,
) -> bool {
    gateway
        .restart(participant, request)
        .is_ok_and(|reply| reply.instance_id == participant.instance_id && reply.accepted)
}
