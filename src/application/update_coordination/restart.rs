//! Peer-first update restart sequencing.

use crate::{
    application::{UpdateExecution, UpdateExecutionStatus},
    domain::{
        InstallationIdentity, InstanceId, ReleaseHighlightAnnouncement, RequestId, SessionId,
        StableVersion,
    },
    ports::{
        runtime::InstanceInfo,
        update::{
            UpdateCancellation, UpdateParticipantGateway, UpdateReplacementExpectation,
            UpdateRestartRequest, UpdateStateStore,
        },
    },
};

use super::UpdateRequest;

pub(super) struct RestartProgress {
    pub(super) initiating: Option<InstanceInfo>,
    pub(super) requested: usize,
    pub(super) accepted: usize,
    pub(super) failed: Vec<InstanceId>,
    pub(super) replacements: Vec<UpdateReplacementExpectation>,
}

pub(super) struct PendingHighlights {
    pub(super) recorded: bool,
    pub(super) announcement: Option<ReleaseHighlightAnnouncement>,
}

pub(super) fn installed_without_restart<G: UpdateParticipantGateway>(
    gateway: &mut G,
    participants: &[InstanceInfo],
    operation_id: RequestId,
    installed: StableVersion,
    state_recorded: bool,
) -> UpdateExecution {
    super::preflight::release_all(gateway, participants, operation_id);
    UpdateExecution {
        operation_id,
        selected_participants: participants.len(),
        prepared_participants: participants.len(),
        restart_requests: 0,
        quiescence_requests: 0,
        quiesced_participants: 0,
        quiescence_failed: Vec::new(),
        restart_accepted: 0,
        replacement_ready: 0,
        replacement_missing: 0,
        restart_failed: participants
            .iter()
            .map(|participant| participant.instance_id)
            .collect(),
        resumable_sessions: participants
            .iter()
            .map(|participant| participant.session_id)
            .collect(),
        convergence_state_recorded: state_recorded,
        status: UpdateExecutionStatus::Installed { version: installed },
    }
}

pub(super) fn participant_needs_restart(
    participant: Option<&InstanceInfo>,
    installed: &StableVersion,
) -> bool {
    participant.is_some_and(|participant| participant.version != installed.to_string())
}

pub(super) fn restart_peers<G: UpdateParticipantGateway>(
    gateway: &mut G,
    mut participants: Vec<InstanceInfo>,
    operation_id: RequestId,
    initiating_instance: InstanceId,
    installed: &StableVersion,
) -> RestartProgress {
    let restart = UpdateRestartRequest {
        operation_id,
        installed_version: installed.clone(),
    };
    let mut requested = 0_usize;
    let mut accepted = 0_usize;
    let initiating = participants
        .iter()
        .position(|participant| participant.instance_id == initiating_instance)
        .map(|index| participants.remove(index));
    let mut failed = Vec::new();
    let mut replacements = Vec::new();
    for participant in &participants {
        if participant.version == installed.to_string() {
            continue;
        }
        requested = requested.saturating_add(1);
        if restart_accepted(gateway, participant, &restart) {
            accepted = accepted.saturating_add(1);
            replacements.push(UpdateReplacementExpectation {
                session_id: participant.session_id,
                previous_instance_id: participant.instance_id,
                previous_pid: participant.pid,
                operation_id,
            });
        } else {
            failed.push(participant.instance_id);
        }
    }
    RestartProgress {
        initiating,
        requested,
        accepted,
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
    operation_id: RequestId,
    initiating_instance: InstanceId,
    installed: &StableVersion,
    restart_allowed: bool,
    requested: &mut usize,
    accepted: &mut usize,
    failed: &mut Vec<InstanceId>,
) {
    let Some(participant) = participant else {
        if !failed.contains(&initiating_instance) {
            failed.push(initiating_instance);
        }
        return;
    };
    if participant.version == installed.to_string() {
        return;
    }
    if !restart_allowed {
        failed.push(participant.instance_id);
        return;
    }
    *requested = (*requested).saturating_add(1);
    let restart = UpdateRestartRequest {
        operation_id,
        installed_version: installed.clone(),
    };
    if restart_accepted(gateway, participant, &restart) {
        *accepted = accepted.saturating_add(1);
    } else {
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
) -> Option<PendingHighlights> {
    let session_id = session_id?;
    if previous == target {
        return Some(PendingHighlights {
            recorded: true,
            announcement: None,
        });
    }
    let Ok(announcement) =
        ReleaseHighlightAnnouncement::pending(session_id, previous.clone(), target.clone())
    else {
        return Some(PendingHighlights {
            recorded: false,
            announcement: None,
        });
    };
    let recorded = state
        .record_release_highlights(installation, announcement.clone())
        .is_ok();
    Some(PendingHighlights {
        recorded,
        announcement: recorded.then_some(announcement),
    })
}

pub(super) fn record_final_restart_state<S: UpdateStateStore>(
    state: &S,
    installation: InstallationIdentity,
    installed: &StableVersion,
    restart_needed: bool,
    initiating_restart_accepted: bool,
) -> bool {
    initiating_restart_accepted
        || state
            .record_restart_state(installation, installed.clone(), restart_needed)
            .is_ok()
}

pub(super) fn record_converged_highlights<S: UpdateStateStore>(
    state: &S,
    request: &UpdateRequest,
    progress: &RestartProgress,
    cancellation: &dyn UpdateCancellation,
    initiating_session: Option<SessionId>,
    previous: &StableVersion,
    installed: &StableVersion,
) -> Option<PendingHighlights> {
    if !progress.failed.is_empty() || progress.initiating.is_none() || cancellation.is_cancelled() {
        return None;
    }
    record_pending_highlights(
        state,
        request.installation,
        initiating_session,
        previous,
        installed,
    )
}

pub(super) fn discard_rejected_announcement<S: UpdateStateStore>(
    state: &S,
    installation: InstallationIdentity,
    pending: Option<&PendingHighlights>,
    initiating_restart_requested: bool,
    initiating_restart_accepted: bool,
) -> bool {
    let Some(pending) = pending else {
        return true;
    };
    if !pending.recorded {
        return false;
    }
    if !initiating_restart_requested || initiating_restart_accepted {
        return true;
    }
    pending.announcement.as_ref().is_none_or(|announcement| {
        state.discard_release_highlights(installation, announcement) == Ok(true)
    })
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
