//! Convergent all-session Homebrew readiness and restart coordination.

use std::time::Duration;

use serde::Serialize;

mod preflight;
mod restart;

use crate::{
    domain::{InstallationIdentity, InstanceId, RequestId, SessionId, StableVersion, Timestamp},
    ports::{
        runtime::InstanceInfo,
        store::STORAGE_PROTOCOL_VERSION,
        update::{
            HomebrewInstaller, UPDATE_CONTROL_PROTOCOL_VERSION, UpdateCancellation, UpdateError,
            UpdateInstanceRegistry, UpdateLockKind, UpdateParticipantGateway, UpdatePrepareRequest,
            UpdateReplacementExpectation, UpdateStateStore,
        },
    },
};

use preflight::{aborted_execution, execution, matching, preflight, release_all};
use restart::{
    PendingHighlights, RestartProgress, initiating_upgrade, record_pending_highlights,
    restart_initiating, restart_peers,
};

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
    /// Compatible participants selected by the pre-install registry scan.
    pub selected_participants: usize,
    /// Preflight participant count.
    pub prepared_participants: usize,
    /// Post-install processes asked to replace themselves.
    pub restart_requests: usize,
    /// Restart requests accepted before process cleanup and replacement.
    pub restart_accepted: usize,
    /// Accepted peer replacements observed at the board-ready boundary.
    pub replacement_ready: usize,
    /// Accepted peer replacements still missing at the bounded observation boundary.
    pub replacement_missing: usize,
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
        let selected = participants.len();
        if participants.is_empty() {
            return Ok(aborted_execution(
                request.operation_id,
                selected,
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
                selected,
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
                    selected,
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
                selected,
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
        let (mut progress, replacement_ready, replacement_missing) =
            self.restart_and_observe_peers(&request, &ready, &installed, cancellation)?;
        let initiator_needs_restart = progress
            .initiating
            .as_ref()
            .is_some_and(|participant| participant.version != installed.to_string());
        let pending_recorded = record_converged_highlights(
            self.state,
            &request,
            &progress,
            cancellation,
            state_recorded.then_some(initiating_session),
            &previous,
            &installed,
        );
        let initiating_restart_allowed = pending_recorded
            .as_ref()
            .is_some_and(|pending| pending.recorded);
        let peer_accepted = progress.accepted;
        restart_initiating(
            self.gateway,
            progress.initiating.as_ref(),
            &ready,
            request.operation_id,
            request.initiating_instance,
            &installed,
            initiating_restart_allowed,
            &mut progress.requested,
            &mut progress.accepted,
            &mut progress.failed,
        );
        let initiating_restart_accepted = progress.accepted > peer_accepted;
        let announcement_state_recorded = discard_rejected_announcement(
            self.state,
            request.installation,
            pending_recorded.as_ref(),
            initiating_restart_allowed && initiator_needs_restart,
            initiating_restart_accepted,
        );
        let restart_needed =
            !progress.failed.is_empty() || (initiating_restart_allowed && initiator_needs_restart);
        let final_state = record_final_restart_state(
            self.state,
            request.installation,
            &installed,
            restart_needed,
            initiating_restart_accepted,
        );
        Ok(UpdateExecution {
            operation_id: request.operation_id,
            selected_participants: ready.len(),
            prepared_participants: ready.len(),
            restart_requests: progress.requested,
            restart_accepted: progress.accepted,
            replacement_ready,
            replacement_missing,
            restart_failed: progress.failed,
            convergence_state_recorded: state_recorded
                && final_state
                && announcement_state_recorded,
            status: UpdateExecutionStatus::Installed { version: installed },
        })
    }

    fn restart_and_observe_peers(
        &mut self,
        request: &UpdateRequest,
        ready: &[InstanceInfo],
        installed: &StableVersion,
        cancellation: &dyn UpdateCancellation,
    ) -> Result<(RestartProgress, usize, usize), UpdateError> {
        let current = matching(self.registry.active_instances()?, request.installation);
        let mut progress = restart_peers(
            self.gateway,
            current,
            ready,
            request.operation_id,
            request.initiating_instance,
            installed,
        );
        let (replacement_ready, replacement_missing) =
            self.observe_peer_replacements(request, ready, installed, &mut progress, cancellation)?;
        Ok((progress, replacement_ready, replacement_missing))
    }

    fn observe_peer_replacements(
        &mut self,
        request: &UpdateRequest,
        ready: &[InstanceInfo],
        installed: &StableVersion,
        progress: &mut RestartProgress,
        cancellation: &dyn UpdateCancellation,
    ) -> Result<(usize, usize), UpdateError> {
        let expected = progress.replacements.len();
        let timeout = if progress.failed.is_empty() {
            REPLACEMENT_TIMEOUT
        } else {
            Duration::ZERO
        };
        let missing = self.wait_for_peer_replacements(
            request,
            ready,
            installed,
            &progress.replacements,
            timeout,
            cancellation,
        )?;
        let missing_count = missing.len();
        for instance in missing {
            if !progress.failed.contains(&instance) {
                progress.failed.push(instance);
            }
        }
        Ok((expected.saturating_sub(missing_count), missing_count))
    }

    fn wait_for_peer_replacements(
        &mut self,
        request: &UpdateRequest,
        ready: &[InstanceInfo],
        installed: &StableVersion,
        replacements: &[UpdateReplacementExpectation],
        timeout: Duration,
        cancellation: &dyn UpdateCancellation,
    ) -> Result<Vec<InstanceId>, UpdateError> {
        let result = self.registry.wait_for_replacements(
            request.installation,
            installed,
            replacements,
            timeout,
            cancellation,
        );
        let Err(error) = result else {
            return result;
        };
        if let Some(initiating) = ready
            .iter()
            .find(|participant| participant.instance_id == request.initiating_instance)
        {
            let _released = self.gateway.release(initiating, request.operation_id);
        }
        let _state_recorded =
            self.state
                .record_restart_state(request.installation, installed.clone(), true);
        Err(error)
    }
}

fn record_final_restart_state<S: UpdateStateStore>(
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

fn record_converged_highlights<S: UpdateStateStore>(
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

fn discard_rejected_announcement<S: UpdateStateStore>(
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

#[cfg(test)]
mod tests;
