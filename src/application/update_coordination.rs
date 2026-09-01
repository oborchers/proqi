//! Convergent all-session Homebrew readiness and restart coordination.

use std::time::Duration;

use serde::Serialize;

mod restart;

use crate::{
    domain::{InstallationIdentity, InstanceId, RequestId, SessionId, StableVersion, Timestamp},
    ports::{
        runtime::InstanceInfo,
        store::STORAGE_PROTOCOL_VERSION,
        update::{
            HomebrewInstaller, UPDATE_CONTROL_PROTOCOL_VERSION, UpdateCancellation, UpdateError,
            UpdateInstanceRegistry, UpdateLockKind, UpdateParticipantGateway, UpdatePrepareReply,
            UpdatePrepareRequest, UpdateStateStore,
        },
    },
};

use restart::{initiating_upgrade, record_pending_highlights, restart_initiating, restart_peers};

const REPLACEMENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether one active process can participate in the current all-session update protocol.
#[must_use]
pub(crate) fn is_compatible_update_participant(
    participant: &InstanceInfo,
    installation: InstallationIdentity,
) -> bool {
    participant.storage_protocol == STORAGE_PROTOCOL_VERSION
        && participant.control_protocol == Some(crate::ports::control::CONTROL_PROTOCOL_VERSION)
        && participant.control_endpoint.is_some()
        && participant.update.as_ref().is_some_and(|context| {
            context.installation_identity == installation
                && context.protocol == UPDATE_CONTROL_PROTOCOL_VERSION
        })
}

/// Final result of one elected update attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UpdateExecution {
    /// Shared attempt identity.
    pub operation_id: RequestId,
    /// Preflight participant count.
    pub prepared_participants: usize,
    /// Post-install processes asked to replace themselves.
    pub restart_requests: usize,
    /// Processes that could not accept a restart request.
    pub restart_failed: Vec<InstanceId>,
    /// Whether convergent state was persisted after installation.
    pub convergence_state_recorded: bool,
    /// Terminal attempt outcome.
    pub status: UpdateExecutionStatus,
}

/// Bounded coordination state without a durable phase transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum UpdateExecutionStatus {
    /// Another process owns the one installer attempt.
    AlreadyInProgress,
    /// Preflight aborted before Homebrew ran.
    Aborted {
        /// Participant that blocked, when one was identifiable.
        blocker: Option<InstanceId>,
        /// Stable content-free reason.
        code: String,
    },
    /// Homebrew succeeded and restart requests were broadcast.
    Installed {
        /// Version independently reported by the installed binary.
        version: StableVersion,
    },
}

/// Coordinates one exact update through injected registry, control, and installer ports.
pub struct UpdateRestartCoordinator<'a, S, R, G, I> {
    state: &'a S,
    registry: &'a R,
    gateway: &'a mut G,
    installer: &'a mut I,
}

struct UpdateRequest {
    operation_id: RequestId,
    initiating_instance: InstanceId,
    installation: InstallationIdentity,
    target: StableVersion,
    deadline: Timestamp,
}

struct PreparedUpgrade {
    request: UpdateRequest,
    ready: Vec<InstanceInfo>,
    initiating_session: SessionId,
    previous: StableVersion,
    installed: StableVersion,
}

impl<'a, S, R, G, I> UpdateRestartCoordinator<'a, S, R, G, I>
where
    S: UpdateStateStore,
    R: UpdateInstanceRegistry,
    G: UpdateParticipantGateway,
    I: HomebrewInstaller,
{
    /// Bind one coordinator to the installation-wide boundaries.
    #[must_use]
    pub const fn new(
        state: &'a S,
        registry: &'a R,
        gateway: &'a mut G,
        installer: &'a mut I,
    ) -> Self {
        Self {
            state,
            registry,
            gateway,
            installer,
        }
    }

    /// Preflight all current participants, install once, rescan, and request replacement.
    ///
    /// # Errors
    ///
    /// Returns only registry, lock, installer, or convergence-state failures. Participant
    /// refusal is an ordinary aborted result and never invokes Homebrew.
    pub fn execute(
        &mut self,
        operation_id: RequestId,
        initiating_instance: InstanceId,
        installation: InstallationIdentity,
        target: &StableVersion,
        deadline: Timestamp,
        cancellation: &dyn UpdateCancellation,
    ) -> Result<UpdateExecution, UpdateError> {
        let Some(_installer_lease) = self
            .state
            .try_lock(installation, UpdateLockKind::Installer)?
        else {
            return Ok(execution(
                operation_id,
                UpdateExecutionStatus::AlreadyInProgress,
            ));
        };
        self.execute_locked(
            UpdateRequest {
                operation_id,
                initiating_instance,
                installation,
                target: target.clone(),
                deadline,
            },
            cancellation,
        )
    }

    fn execute_locked(
        &mut self,
        request: UpdateRequest,
        cancellation: &dyn UpdateCancellation,
    ) -> Result<UpdateExecution, UpdateError> {
        let participants = matching(self.registry.active_instances()?, request.installation);
        if participants.is_empty() {
            return Ok(aborted_execution(
                request.operation_id,
                0,
                None,
                "no_compatible_participants",
            ));
        }
        if !participants
            .iter()
            .any(|participant| participant.instance_id == request.initiating_instance)
        {
            return Ok(aborted_execution(
                request.operation_id,
                0,
                Some(request.initiating_instance),
                "coordinator_not_registered",
            ));
        }
        let prepare = UpdatePrepareRequest {
            operation_id: request.operation_id,
            target_version: request.target.clone(),
            installation_identity: request.installation,
            deadline: request.deadline,
        };
        let ready = match preflight(self.gateway, &participants, &prepare) {
            Ok(ready) => ready,
            Err(failure) => {
                release_all(self.gateway, &failure.ready, request.operation_id);
                return Ok(aborted_execution(
                    request.operation_id,
                    failure.ready.len(),
                    failure.blocker,
                    &failure.code,
                ));
            }
        };
        let Some((initiating_session, previous)) =
            initiating_upgrade(&ready, request.initiating_instance)
        else {
            release_all(self.gateway, &ready, request.operation_id);
            return Ok(aborted_execution(
                request.operation_id,
                ready.len(),
                Some(request.initiating_instance),
                "invalid_coordinator_version",
            ));
        };
        let installed = match self.installer.upgrade(&request.target) {
            Ok(installed) => installed,
            Err(error) => {
                release_all(self.gateway, &ready, request.operation_id);
                return Err(error);
            }
        };
        if installed != request.target {
            release_all(self.gateway, &ready, request.operation_id);
            return Err(UpdateError::InstallerFailed);
        }
        self.restart_installed(
            PreparedUpgrade {
                request,
                ready,
                initiating_session,
                previous,
                installed,
            },
            cancellation,
        )
    }

    fn restart_installed(
        &mut self,
        upgrade: PreparedUpgrade,
        cancellation: &dyn UpdateCancellation,
    ) -> Result<UpdateExecution, UpdateError> {
        let PreparedUpgrade {
            request,
            ready,
            initiating_session,
            previous,
            installed,
        } = upgrade;
        let state_recorded = self
            .state
            .record_restart_state(request.installation, installed.clone(), true)
            .is_ok();
        let current = matching(self.registry.active_instances()?, request.installation);
        let mut progress = restart_peers(
            self.gateway,
            current,
            &ready,
            request.operation_id,
            request.initiating_instance,
            &installed,
        );
        if progress.failed.is_empty() {
            progress.failed = self.registry.wait_for_replacements(
                request.installation,
                &installed,
                &progress.replacements,
                REPLACEMENT_TIMEOUT,
                cancellation,
            )?;
        }
        let pending_recorded = if progress.failed.is_empty() && !cancellation.is_cancelled() {
            record_pending_highlights(
                self.state,
                request.installation,
                Some(initiating_session),
                &previous,
                &installed,
            )
        } else {
            None
        };
        restart_initiating(
            self.gateway,
            progress.initiating.as_ref(),
            &ready,
            request.operation_id,
            request.initiating_instance,
            &installed,
            pending_recorded,
            &mut progress.requested,
            &mut progress.failed,
        );
        let final_state = self
            .state
            .record_restart_state(
                request.installation,
                installed.clone(),
                !progress.failed.is_empty(),
            )
            .is_ok();
        Ok(UpdateExecution {
            operation_id: request.operation_id,
            prepared_participants: ready.len(),
            restart_requests: progress.requested,
            restart_failed: progress.failed,
            convergence_state_recorded: state_recorded
                && final_state
                && pending_recorded.unwrap_or(true),
            status: UpdateExecutionStatus::Installed { version: installed },
        })
    }
}

struct PreflightFailure {
    ready: Vec<InstanceInfo>,
    blocker: Option<InstanceId>,
    code: String,
}

fn preflight<G: UpdateParticipantGateway>(
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

fn aborted_execution(
    operation_id: RequestId,
    prepared_participants: usize,
    blocker: Option<InstanceId>,
    code: &str,
) -> UpdateExecution {
    UpdateExecution {
        operation_id,
        prepared_participants,
        restart_requests: 0,
        restart_failed: Vec::new(),
        convergence_state_recorded: true,
        status: UpdateExecutionStatus::Aborted {
            blocker,
            code: code.to_owned(),
        },
    }
}

fn matching(instances: Vec<InstanceInfo>, installation: InstallationIdentity) -> Vec<InstanceInfo> {
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

fn release_all<G: UpdateParticipantGateway>(
    gateway: &mut G,
    participants: &[InstanceInfo],
    operation_id: RequestId,
) {
    for participant in participants {
        let _released = gateway.release(participant, operation_id);
    }
}

fn execution(operation_id: RequestId, status: UpdateExecutionStatus) -> UpdateExecution {
    UpdateExecution {
        operation_id,
        prepared_participants: 0,
        restart_requests: 0,
        restart_failed: Vec::new(),
        convergence_state_recorded: true,
        status,
    }
}

#[cfg(test)]
mod tests;
