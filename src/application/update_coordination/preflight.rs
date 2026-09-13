//! Participant selection, preparation, and aborted-result construction.

use crate::{
    application::{UpdateExecution, UpdateExecutionStatus, is_compatible_update_participant},
    domain::{InstallationIdentity, InstanceId, RequestId, SessionId, StableVersion, Timestamp},
    ports::{
        environment::Clock,
        runtime::InstanceInfo,
        update::{
            UpdateError, UpdateInstanceRegistry, UpdateParticipantGateway, UpdatePrepareReply,
            UpdatePrepareRequest,
        },
    },
};

use super::{UpdateRequest, restart::initiating_upgrade};

pub(super) struct PreparedUpgrade {
    pub(super) request: UpdateRequest,
    pub(super) ready: Vec<InstanceInfo>,
    pub(super) initiating_session: SessionId,
    pub(super) previous: StableVersion,
    pub(super) installed: StableVersion,
    pub(super) state_recorded: bool,
}

pub(super) enum InstalledPreparation {
    Ready(PreparedUpgrade),
    Terminal(UpdateExecution),
}

pub(super) struct PreparedParticipants {
    pub(super) selected: Vec<InstanceInfo>,
    pub(super) ready: Vec<InstanceInfo>,
    pub(super) initiating_session: SessionId,
    pub(super) previous: StableVersion,
}

pub(super) enum InitialPreparation {
    Ready(PreparedParticipants),
    Terminal(UpdateExecution),
}

pub(super) fn prepare_after_install<R: UpdateInstanceRegistry, G: UpdateParticipantGateway>(
    registry: &R,
    gateway: &mut G,
    clock: &dyn Clock,
    mut request: UpdateRequest,
    preparation_window_millis: i64,
    prepared: PreparedParticipants,
    installed_state: (StableVersion, bool),
) -> Result<InstalledPreparation, UpdateError> {
    let (installed, state_recorded) = installed_state;
    let PreparedParticipants {
        selected: participants,
        initiating_session,
        previous,
        ..
    } = prepared;
    request.deadline = Timestamp::from_millis(
        clock
            .now()
            .as_millis()
            .saturating_add(preparation_window_millis),
    );
    let current = matching(registry.active_instances()?, request.installation);
    let selected_after_rescan = participants.len().saturating_add(
        current
            .iter()
            .filter(|participant| {
                !participants
                    .iter()
                    .any(|initial| initial.instance_id == participant.instance_id)
            })
            .count(),
    );
    let prepare = UpdatePrepareRequest {
        operation_id: request.operation_id,
        target_version: request.target.clone(),
        installation_identity: request.installation,
        deadline: request.deadline,
    };
    let ready = match preflight(gateway, &current, &prepare) {
        Ok(ready)
            if ready
                .iter()
                .any(|participant| participant.instance_id == request.initiating_instance) =>
        {
            ready
        }
        Ok(ready) => {
            release_all(gateway, &ready, request.operation_id);
            return Ok(InstalledPreparation::Terminal(
                installed_preparation_failed(
                    &request,
                    selected_after_rescan,
                    ready.len(),
                    &current,
                    &participants,
                    installed,
                    state_recorded,
                ),
            ));
        }
        Err(failure) => {
            release_all(gateway, &failure.ready, request.operation_id);
            return Ok(InstalledPreparation::Terminal(
                installed_preparation_failed(
                    &request,
                    selected_after_rescan,
                    failure.ready.len(),
                    &current,
                    &participants,
                    installed,
                    state_recorded,
                ),
            ));
        }
    };
    Ok(InstalledPreparation::Ready(PreparedUpgrade {
        request,
        ready,
        initiating_session,
        previous,
        installed,
        state_recorded,
    }))
}

pub(super) fn prepare_current<R: UpdateInstanceRegistry, G: UpdateParticipantGateway>(
    registry: &R,
    gateway: &mut G,
    request: &UpdateRequest,
) -> Result<InitialPreparation, UpdateError> {
    let participants = matching(registry.active_instances()?, request.installation);
    let selected = participants.len();
    if participants.is_empty() {
        return Ok(InitialPreparation::Terminal(aborted_execution(
            request.operation_id,
            selected,
            0,
            None,
            "no_compatible_participants",
        )));
    }
    if !participants
        .iter()
        .any(|participant| participant.instance_id == request.initiating_instance)
    {
        return Ok(InitialPreparation::Terminal(aborted_execution(
            request.operation_id,
            selected,
            0,
            Some(request.initiating_instance),
            "coordinator_not_registered",
        )));
    }
    let prepare = UpdatePrepareRequest {
        operation_id: request.operation_id,
        target_version: request.target.clone(),
        installation_identity: request.installation,
        deadline: request.deadline,
    };
    let ready = match preflight(gateway, &participants, &prepare) {
        Ok(ready) => ready,
        Err(failure) => {
            release_all(gateway, &failure.ready, request.operation_id);
            return Ok(InitialPreparation::Terminal(aborted_execution(
                request.operation_id,
                selected,
                failure.ready.len(),
                failure.blocker,
                &failure.code,
            )));
        }
    };
    let Some((initiating_session, previous)) =
        initiating_upgrade(&ready, request.initiating_instance)
    else {
        release_all(gateway, &ready, request.operation_id);
        return Ok(InitialPreparation::Terminal(aborted_execution(
            request.operation_id,
            selected,
            ready.len(),
            Some(request.initiating_instance),
            "invalid_coordinator_version",
        )));
    };
    Ok(InitialPreparation::Ready(PreparedParticipants {
        selected: participants,
        ready,
        initiating_session,
        previous,
    }))
}

pub(super) struct PreflightFailure {
    pub(super) ready: Vec<InstanceInfo>,
    pub(super) blocker: Option<InstanceId>,
    pub(super) code: String,
}

pub(super) fn preflight<G: UpdateParticipantGateway>(
    gateway: &mut G,
    participants: &[InstanceInfo],
    request: &UpdatePrepareRequest,
) -> Result<Vec<InstanceInfo>, PreflightFailure> {
    let mut ready = Vec::new();
    for participant in participants {
        let reply = gateway.prepare(participant, request);
        if let Some(status) = validate_reply(participant, reply) {
            let UpdateExecutionStatus::Aborted { blocker, code } = status else {
                return Err(PreflightFailure {
                    ready,
                    blocker: Some(participant.instance_id),
                    code: "invalid_preflight_status".to_owned(),
                });
            };
            return Err(PreflightFailure {
                ready,
                blocker,
                code,
            });
        }
        ready.push(participant.clone());
    }
    Ok(ready)
}

pub(super) fn aborted_execution(
    operation_id: RequestId,
    selected_participants: usize,
    prepared_participants: usize,
    blocker: Option<InstanceId>,
    code: &str,
) -> UpdateExecution {
    UpdateExecution {
        operation_id,
        selected_participants,
        prepared_participants,
        restart_requests: 0,
        quiescence_requests: 0,
        quiesced_participants: 0,
        quiescence_failed: Vec::new(),
        restart_accepted: 0,
        replacement_ready: 0,
        replacement_missing: 0,
        restart_failed: Vec::new(),
        resumable_sessions: Vec::new(),
        convergence_state_recorded: true,
        status: UpdateExecutionStatus::Aborted {
            blocker,
            code: code.to_owned(),
        },
    }
}

pub(super) fn installed_preparation_failed(
    request: &UpdateRequest,
    selected_participants: usize,
    prepared_participants: usize,
    observed: &[InstanceInfo],
    initial: &[InstanceInfo],
    installed: StableVersion,
    state_recorded: bool,
) -> UpdateExecution {
    let mut affected = initial.to_vec();
    for participant in observed {
        if !affected
            .iter()
            .any(|known| known.instance_id == participant.instance_id)
        {
            affected.push(participant.clone());
        }
    }
    UpdateExecution {
        operation_id: request.operation_id,
        selected_participants,
        prepared_participants,
        restart_requests: 0,
        quiescence_requests: 0,
        quiesced_participants: 0,
        quiescence_failed: Vec::new(),
        restart_accepted: 0,
        replacement_ready: 0,
        replacement_missing: 0,
        restart_failed: affected
            .iter()
            .map(|participant| participant.instance_id)
            .collect(),
        resumable_sessions: affected
            .iter()
            .map(|participant| participant.session_id)
            .collect(),
        convergence_state_recorded: state_recorded,
        status: UpdateExecutionStatus::Installed { version: installed },
    }
}

pub(super) fn matching(
    instances: Vec<InstanceInfo>,
    installation: InstallationIdentity,
) -> Vec<InstanceInfo> {
    instances
        .into_iter()
        .filter(|info| is_compatible_update_participant(info, installation))
        .collect()
}

fn validate_reply(
    participant: &InstanceInfo,
    reply: Result<UpdatePrepareReply, UpdateError>,
) -> Option<UpdateExecutionStatus> {
    match reply {
        Ok(UpdatePrepareReply::Ready {
            instance_id,
            session_id,
        }) if instance_id == participant.instance_id && session_id == participant.session_id => {
            None
        }
        Ok(UpdatePrepareReply::Blocked { instance_id, code }) => {
            Some(UpdateExecutionStatus::Aborted {
                blocker: Some(instance_id),
                code,
            })
        }
        Ok(UpdatePrepareReply::Ready { .. }) => Some(UpdateExecutionStatus::Aborted {
            blocker: Some(participant.instance_id),
            code: "invalid_readiness_receipt".to_owned(),
        }),
        Err(_) => Some(UpdateExecutionStatus::Aborted {
            blocker: Some(participant.instance_id),
            code: "participant_unavailable".to_owned(),
        }),
    }
}

pub(super) fn release_all<G: UpdateParticipantGateway>(
    gateway: &mut G,
    participants: &[InstanceInfo],
    operation_id: RequestId,
) {
    for participant in participants {
        let _released = gateway.release(participant, operation_id);
    }
}

pub(super) fn execution(operation_id: RequestId, status: UpdateExecutionStatus) -> UpdateExecution {
    UpdateExecution {
        operation_id,
        selected_participants: 0,
        prepared_participants: 0,
        restart_requests: 0,
        quiescence_requests: 0,
        quiesced_participants: 0,
        quiescence_failed: Vec::new(),
        restart_accepted: 0,
        replacement_ready: 0,
        replacement_missing: 0,
        restart_failed: Vec::new(),
        resumable_sessions: Vec::new(),
        convergence_state_recorded: true,
        status,
    }
}
