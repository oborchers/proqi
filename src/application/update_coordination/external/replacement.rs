//! Exact durable replacement construction and observation helpers.

use crate::{
    domain::{
        ExternalRestartExpectation, ExternalRestartPending, InstanceId, RequestId, StableVersion,
    },
    ports::{
        runtime::InstanceInfo,
        update::{
            UpdateError, UpdateParticipantGateway, UpdateReplacementExpectation,
            UpdateRestartRequest,
        },
    },
};

use super::{
    ExternalUpgradeBlocker, ExternalUpgradeBlockerReason, ExternalUpgradeFailure,
    participants::blocker,
};

pub(super) struct ExternalRestart {
    pub(super) failed: Vec<InstanceId>,
}

pub(super) fn restart_all<G: UpdateParticipantGateway>(
    gateway: &mut G,
    participants: &[InstanceInfo],
    operation_id: RequestId,
    current: &StableVersion,
) -> ExternalRestart {
    let request = UpdateRestartRequest {
        operation_id,
        installed_version: current.clone(),
    };
    let mut failed = Vec::new();
    for participant in participants {
        match gateway.restart(participant, &request) {
            Ok(reply) if reply.instance_id == participant.instance_id && reply.accepted => {}
            Ok(_) | Err(_) => failed.push(participant.instance_id),
        }
    }
    ExternalRestart { failed }
}

pub(super) fn pending_restart(
    operation_id: RequestId,
    current: &StableVersion,
    participants: &[InstanceInfo],
) -> Result<ExternalRestartPending, ExternalUpgradeFailure> {
    let expectations = participants
        .iter()
        .map(|participant| {
            StableVersion::parse(&participant.version)
                .map(|version| {
                    ExternalRestartExpectation::new(
                        participant.session_id,
                        participant.instance_id,
                        participant.pid,
                        version,
                    )
                })
                .map_err(|error| UpdateError::State(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    ExternalRestartPending::new(current.clone(), operation_id, expectations)
        .map_err(|error| ExternalUpgradeFailure::Update(UpdateError::State(error.to_string())))
}

pub(super) fn replacement_expectations(
    pending: &ExternalRestartPending,
) -> Vec<UpdateReplacementExpectation> {
    pending
        .expectations()
        .iter()
        .map(|expectation| UpdateReplacementExpectation {
            session_id: expectation.session_id(),
            previous_instance_id: expectation.previous_instance_id(),
            previous_pid: expectation.previous_pid(),
            operation_id: pending.operation_id(),
        })
        .collect()
}

pub(super) fn pending_replacement_blockers(
    pending: &ExternalRestartPending,
    missing: &[InstanceId],
) -> Vec<ExternalUpgradeBlocker> {
    pending
        .expectations()
        .iter()
        .filter(|expectation| missing.contains(&expectation.previous_instance_id()))
        .map(|expectation| ExternalUpgradeBlocker {
            instance_id: expectation.previous_instance_id(),
            session_id: expectation.session_id(),
            version: Some(expectation.previous_version().clone()),
            reason: ExternalUpgradeBlockerReason::ReplacementIncomplete,
        })
        .collect()
}

pub(super) fn replacement_blockers(
    participants: &[InstanceInfo],
    failed: &[InstanceId],
    missing: &[InstanceId],
) -> Vec<ExternalUpgradeBlocker> {
    participants
        .iter()
        .filter(|participant| {
            failed.contains(&participant.instance_id) || missing.contains(&participant.instance_id)
        })
        .map(|participant| {
            blocker(
                participant,
                ExternalUpgradeBlockerReason::ReplacementIncomplete,
            )
        })
        .collect()
}
