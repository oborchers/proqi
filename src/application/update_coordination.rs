//! Convergent all-session installation readiness and restart coordination.

use std::time::Duration;

use serde::Serialize;

mod preflight;
mod quiescence;
mod restart;

use crate::{
    domain::{InstallationIdentity, InstanceId, RequestId, SessionId, StableVersion, Timestamp},
    ports::{
        environment::Clock,
        runtime::InstanceInfo,
        store::STORAGE_PROTOCOL_VERSION,
        update::{
            UPDATE_CONTROL_PROTOCOL_VERSION, UpdateCancellation, UpdateError, UpdateInstaller,
            UpdateInstanceRegistry, UpdateLockKind, UpdateParticipantGateway, UpdateStateStore,
        },
    },
};

use preflight::{
    InitialPreparation, InstalledPreparation, PreparedUpgrade, execution, prepare_after_install,
    prepare_current, release_all,
};
use quiescence::{QuiescenceProgress, quiesce_prepared};
use restart::{
    RestartProgress, discard_rejected_announcement, installed_without_restart,
    participant_needs_restart, record_converged_highlights, record_final_restart_state,
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
    /// Compatible participants selected across the pre-install and post-install scans.
    pub selected_participants: usize,
    /// Preflight participant count.
    pub prepared_participants: usize,
    /// Post-install processes asked to replace themselves.
    pub restart_requests: usize,
    /// Old prepared processes asked to enter irreversible schema quiescence.
    pub quiescence_requests: usize,
    /// Exact prepared processes that acknowledged shared schema lease release.
    pub quiesced_participants: usize,
    /// Processes whose quiescence could not be verified.
    pub quiescence_failed: Vec<InstanceId>,
    /// Restart requests accepted before process cleanup and replacement.
    pub restart_accepted: usize,
    /// Accepted peer replacements observed at the board-ready boundary.
    pub replacement_ready: usize,
    /// Accepted peer replacements still missing at the bounded observation boundary.
    pub replacement_missing: usize,
    /// Processes that could not accept a restart request.
    pub restart_failed: Vec<InstanceId>,
    /// Exact durable sessions that remain manually resumable after an incomplete replacement.
    pub resumable_sessions: Vec<SessionId>,
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
    /// Preflight aborted before the verified installer ran.
    Aborted {
        /// Participant that blocked, when one was identifiable.
        blocker: Option<InstanceId>,
        /// Stable content-free reason.
        code: String,
    },
    /// Installation succeeded and restart requests were broadcast.
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
    clock: &'a dyn Clock,
}

struct UpdateRequest {
    operation_id: RequestId,
    initiating_instance: InstanceId,
    installation: InstallationIdentity,
    target: StableVersion,
    deadline: Timestamp,
}

struct RestartedUpgrade {
    upgrade: PreparedUpgrade,
    quiescence: QuiescenceProgress,
    progress: RestartProgress,
    replacement_ready: usize,
    replacement_missing: usize,
}

impl<'a, S, R, G, I> UpdateRestartCoordinator<'a, S, R, G, I>
where
    S: UpdateStateStore,
    R: UpdateInstanceRegistry,
    G: UpdateParticipantGateway,
    I: UpdateInstaller,
{
    /// Bind one coordinator to the installation-wide boundaries.
    #[must_use]
    pub const fn new(
        state: &'a S,
        registry: &'a R,
        gateway: &'a mut G,
        installer: &'a mut I,
        clock: &'a dyn Clock,
    ) -> Self {
        Self {
            state,
            registry,
            gateway,
            installer,
            clock,
        }
    }

    /// Preflight all current participants, install once, rescan, and request replacement.
    ///
    /// # Errors
    ///
    /// Returns only registry, lock, installer, or convergence-state failures. Participant
    /// refusal is an ordinary aborted result and never invokes an installer.
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
        let Some(convergence_lease) = self
            .state
            .try_lock(installation, UpdateLockKind::Convergence)?
        else {
            return Ok(execution(
                operation_id,
                UpdateExecutionStatus::AlreadyInProgress,
            ));
        };
        let preparation_window_millis = deadline
            .as_millis()
            .saturating_sub(self.clock.now().as_millis())
            .max(0);
        self.execute_locked(
            UpdateRequest {
                operation_id,
                initiating_instance,
                installation,
                target: target.clone(),
                deadline,
            },
            preparation_window_millis,
            convergence_lease,
            cancellation,
        )
    }

    fn execute_locked(
        &mut self,
        request: UpdateRequest,
        preparation_window_millis: i64,
        convergence_lease: Box<dyn crate::ports::update::UpdateLease>,
        cancellation: &dyn UpdateCancellation,
    ) -> Result<UpdateExecution, UpdateError> {
        let prepared = match prepare_current(self.registry, self.gateway, &request)? {
            InitialPreparation::Ready(prepared) => prepared,
            InitialPreparation::Terminal(execution) => return Ok(execution),
        };
        release_all(self.gateway, &prepared.ready, request.operation_id);
        let installed = self.installer.upgrade(&request.target)?;
        if installed != request.target {
            return Err(UpdateError::InstallerFailed);
        }
        let state_recorded = self
            .state
            .record_restart_state(request.installation, installed.clone(), true)
            .is_ok();
        match prepare_after_install(
            self.registry,
            self.gateway,
            self.clock,
            request,
            preparation_window_millis,
            prepared,
            (installed, state_recorded),
        )? {
            InstalledPreparation::Ready(upgrade) => {
                Ok(self.restart_installed(upgrade, convergence_lease, cancellation))
            }
            InstalledPreparation::Terminal(execution) => Ok(execution),
        }
    }

    fn restart_installed(
        &mut self,
        upgrade: PreparedUpgrade,
        convergence_lease: Box<dyn crate::ports::update::UpdateLease>,
        cancellation: &dyn UpdateCancellation,
    ) -> UpdateExecution {
        if cancellation.is_cancelled() || !upgrade.state_recorded {
            return installed_without_restart(
                self.gateway,
                &upgrade.ready,
                upgrade.request.operation_id,
                upgrade.installed,
                upgrade.state_recorded,
            );
        }
        let quiescence = quiesce_prepared(
            self.gateway,
            &upgrade.ready,
            upgrade.request.operation_id,
            &upgrade.installed,
        );
        drop(convergence_lease);
        let mut restartable = quiescence.quiesced.clone();
        restartable.extend(quiescence.unchanged.iter().cloned());
        let (mut progress, replacement_ready, replacement_missing) = self
            .restart_and_observe_peers(
                &upgrade.request,
                &restartable,
                &upgrade.installed,
                cancellation,
            );
        for instance in &quiescence.failed {
            if !progress.failed.contains(instance) {
                progress.failed.push(*instance);
            }
        }
        self.finish_restart(
            RestartedUpgrade {
                upgrade,
                quiescence,
                progress,
                replacement_ready,
                replacement_missing,
            },
            cancellation,
        )
    }

    #[expect(
        clippy::too_many_lines,
        reason = "final restart accounting remains one ordered state transition"
    )]
    fn finish_restart(
        &mut self,
        restarted: RestartedUpgrade,
        cancellation: &dyn UpdateCancellation,
    ) -> UpdateExecution {
        let RestartedUpgrade {
            upgrade:
                PreparedUpgrade {
                    request,
                    ready,
                    initiating_session,
                    previous,
                    installed,
                    state_recorded,
                },
            quiescence,
            mut progress,
            replacement_ready,
            replacement_missing,
        } = restarted;
        let pending_recorded = record_converged_highlights(
            self.state,
            &request,
            &progress,
            cancellation,
            Some(initiating_session),
            &previous,
            &installed,
        );
        let initiating_restart_allowed = quiescence
            .quiesced
            .iter()
            .any(|participant| participant.instance_id == request.initiating_instance);
        let initiator_needs_restart =
            participant_needs_restart(progress.initiating.as_ref(), &installed);
        let peer_accepted = progress.accepted;
        restart_initiating(
            self.gateway,
            progress.initiating.as_ref(),
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
            initiating_restart_allowed,
            initiating_restart_accepted,
        );
        let restart_needed = !progress.failed.is_empty()
            || (initiator_needs_restart && !initiating_restart_accepted);
        let final_state = record_final_restart_state(
            self.state,
            request.installation,
            &installed,
            restart_needed,
            initiating_restart_accepted,
        );
        let resumable_sessions = ready
            .iter()
            .filter(|participant| progress.failed.contains(&participant.instance_id))
            .map(|participant| participant.session_id)
            .collect();
        UpdateExecution {
            operation_id: request.operation_id,
            selected_participants: ready.len(),
            prepared_participants: ready.len(),
            restart_requests: progress.requested,
            quiescence_requests: quiescence.requested,
            quiesced_participants: quiescence.quiesced.len(),
            quiescence_failed: quiescence.failed,
            restart_accepted: progress.accepted,
            replacement_ready,
            replacement_missing,
            restart_failed: progress.failed,
            resumable_sessions,
            convergence_state_recorded: state_recorded
                && final_state
                && announcement_state_recorded
                && pending_recorded
                    .as_ref()
                    .is_none_or(|pending| pending.recorded),
            status: UpdateExecutionStatus::Installed { version: installed },
        }
    }

    fn restart_and_observe_peers(
        &mut self,
        request: &UpdateRequest,
        quiesced: &[InstanceInfo],
        installed: &StableVersion,
        cancellation: &dyn UpdateCancellation,
    ) -> (RestartProgress, usize, usize) {
        let mut progress = restart_peers(
            self.gateway,
            quiesced.to_vec(),
            request.operation_id,
            request.initiating_instance,
            installed,
        );
        let (replacement_ready, replacement_missing) =
            self.observe_peer_replacements(request, installed, &mut progress, cancellation);
        (progress, replacement_ready, replacement_missing)
    }

    fn observe_peer_replacements(
        &mut self,
        request: &UpdateRequest,
        installed: &StableVersion,
        progress: &mut RestartProgress,
        cancellation: &dyn UpdateCancellation,
    ) -> (usize, usize) {
        let expected = progress.replacements.len();
        let timeout = if progress.failed.is_empty() {
            REPLACEMENT_TIMEOUT
        } else {
            Duration::ZERO
        };
        let missing = self
            .registry
            .wait_for_replacements(
                request.installation,
                installed,
                &progress.replacements,
                timeout,
                cancellation,
            )
            .unwrap_or_else(|_| {
                progress
                    .replacements
                    .iter()
                    .map(|replacement| replacement.previous_instance_id)
                    .collect()
            });
        let missing_count = missing.len();
        for instance in missing {
            if !progress.failed.contains(&instance) {
                progress.failed.push(instance);
            }
        }
        (expected.saturating_sub(missing_count), missing_count)
    }
}

#[cfg(test)]
mod tests;
