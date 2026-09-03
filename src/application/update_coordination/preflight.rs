//! Participant selection, preparation, and aborted-result construction.

use crate::{
    application::{UpdateExecution, UpdateExecutionStatus, is_compatible_update_participant},
    domain::{InstallationIdentity, InstanceId, RequestId},
    ports::{
        runtime::InstanceInfo,
        update::{UpdateError, UpdateParticipantGateway, UpdatePrepareReply, UpdatePrepareRequest},
    },
};

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
        restart_accepted: 0,
        replacement_ready: 0,
        replacement_missing: 0,
        restart_failed: Vec::new(),
        convergence_state_recorded: true,
        status: UpdateExecutionStatus::Aborted {
            blocker,
            code: code.to_owned(),
        },
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
        restart_accepted: 0,
        replacement_ready: 0,
        replacement_missing: 0,
        restart_failed: Vec::new(),
        convergence_state_recorded: true,
        status,
    }
}
