//! Typed live-participant classification and private blocker projection.

use crate::{
    domain::{
        EXTERNAL_RESTART_MAX_EXPECTATIONS, InstallationIdentity, InstallationKind, InstanceId,
        StableVersion,
    },
    ports::{
        control::CONTROL_PROTOCOL_VERSION, runtime::InstanceInfo, store::STORAGE_PROTOCOL_VERSION,
        update::UPDATE_CONTROL_PROTOCOL_VERSION,
    },
};

use super::{ExternalUpgradeBlocker, ExternalUpgradeBlockerReason};

pub(super) struct ExternalPlan {
    pub(super) compatible_older: Vec<InstanceInfo>,
    pub(super) blockers: Vec<ExternalUpgradeBlocker>,
}

pub(super) struct ExternalCohortCapacity {
    pub(super) blockers: Vec<ExternalUpgradeBlocker>,
    pub(super) participant_count: usize,
    pub(super) maximum: usize,
}

pub(super) fn capacity_exceeded(participants: &[InstanceInfo]) -> Option<ExternalCohortCapacity> {
    let participant_count = participants.len();
    (participant_count > EXTERNAL_RESTART_MAX_EXPECTATIONS).then(|| {
        let diagnostic_limit = EXTERNAL_RESTART_MAX_EXPECTATIONS.saturating_add(1);
        ExternalCohortCapacity {
            blockers: blockers(
                &participants[..diagnostic_limit.min(participant_count)],
                ExternalUpgradeBlockerReason::CohortCapacityExceeded,
            ),
            participant_count,
            maximum: EXTERNAL_RESTART_MAX_EXPECTATIONS,
        }
    })
}

pub(super) fn plan(
    instances: Vec<InstanceInfo>,
    installation: InstallationIdentity,
    installation_kind: InstallationKind,
    current: &StableVersion,
) -> ExternalPlan {
    let mut compatible_older = Vec::new();
    let mut found_blockers = Vec::new();
    for instance in instances {
        let Ok(version) = StableVersion::parse(&instance.version) else {
            found_blockers.push(blocker_with_version(
                &instance,
                None,
                ExternalUpgradeBlockerReason::IncompatibleWriter,
            ));
            continue;
        };
        if version > *current {
            found_blockers.push(blocker(
                &instance,
                ExternalUpgradeBlockerReason::NewerRuntime,
            ));
        } else if version == *current {
            if instance.storage_protocol != STORAGE_PROTOCOL_VERSION {
                found_blockers.push(blocker(
                    &instance,
                    ExternalUpgradeBlockerReason::IncompatibleWriter,
                ));
            }
        } else if installation_kind == InstallationKind::SourceOrUnknown {
            found_blockers.push(blocker(
                &instance,
                ExternalUpgradeBlockerReason::RestartUnsupported,
            ));
        } else if supports_external_quiescence(&instance, installation) {
            compatible_older.push(instance);
        } else {
            found_blockers.push(blocker(
                &instance,
                ExternalUpgradeBlockerReason::OlderIncompatible,
            ));
        }
    }
    ExternalPlan {
        compatible_older,
        blockers: found_blockers,
    }
}

fn supports_external_quiescence(
    participant: &InstanceInfo,
    installation: InstallationIdentity,
) -> bool {
    participant.storage_protocol <= STORAGE_PROTOCOL_VERSION
        && participant.control_protocol == Some(CONTROL_PROTOCOL_VERSION)
        && participant.control_endpoint.is_some()
        && participant.update.as_ref().is_some_and(|context| {
            context.installation_identity == installation
                && context.protocol == UPDATE_CONTROL_PROTOCOL_VERSION
        })
}

pub(super) fn blockers(
    participants: &[InstanceInfo],
    reason: ExternalUpgradeBlockerReason,
) -> Vec<ExternalUpgradeBlocker> {
    participants
        .iter()
        .map(|participant| blocker(participant, reason))
        .collect()
}

pub(super) fn preparation_blockers(
    participants: &[InstanceInfo],
    failed: Option<InstanceId>,
) -> Vec<ExternalUpgradeBlocker> {
    let exact = failed.map_or_else(Vec::new, |instance| {
        blockers_by_instance(
            participants,
            &[instance],
            ExternalUpgradeBlockerReason::PreparationFailed,
        )
    });
    if exact.is_empty() {
        blockers(
            participants,
            ExternalUpgradeBlockerReason::PreparationFailed,
        )
    } else {
        exact
    }
}

pub(super) fn blockers_by_instance(
    participants: &[InstanceInfo],
    instances: &[InstanceId],
    reason: ExternalUpgradeBlockerReason,
) -> Vec<ExternalUpgradeBlocker> {
    participants
        .iter()
        .filter(|participant| instances.contains(&participant.instance_id))
        .map(|participant| blocker(participant, reason))
        .collect()
}

pub(super) fn blocker(
    participant: &InstanceInfo,
    reason: ExternalUpgradeBlockerReason,
) -> ExternalUpgradeBlocker {
    blocker_with_version(
        participant,
        StableVersion::parse(&participant.version).ok(),
        reason,
    )
}

fn blocker_with_version(
    participant: &InstanceInfo,
    version: Option<StableVersion>,
    reason: ExternalUpgradeBlockerReason,
) -> ExternalUpgradeBlocker {
    ExternalUpgradeBlocker {
        instance_id: participant.instance_id,
        session_id: participant.session_id,
        version,
        reason,
    }
}
