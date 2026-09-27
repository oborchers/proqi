//! Startup admission across in-app and externally installed upgrades.

use std::{path::Path, time::Duration};

use crate::{
    adapters::{
        control::LocalUpdateControlClient,
        runtime::{FileRuntimeCoordinator, SystemIdGenerator},
        update::FileUpdateStateStore,
    },
    application::{ExternalUpgradeCoordinator, admit_pending_external_resume},
    domain::{
        ExternalRestartPending, Installation, InstallationIdentity, InstalledVersionRelation,
        SessionId, StableVersion, Timestamp,
    },
    ports::{
        environment::IdGenerator,
        runtime::UpdateReplacementContext,
        update::{
            ExternalUpgradeAdoptionAuthority, UpdateError, UpdateLease, UpdateLockKind,
            UpdateStateStore as _,
        },
    },
};

use super::super::output::CliError;

const STARTUP_ADMISSION_TIMEOUT: Duration = Duration::from_secs(10);
const EXTERNAL_PREPARATION_WINDOW_MILLIS: i64 = 15_000;

mod adoption;
mod deferred;
mod diagnostics;

use adoption::adopt_external_candidate;
pub(in crate::cli::runtime) use deferred::StartupAdmission;
use diagnostics::{
    convergence_active, external_error, obsolete_error, pending_target_error, update_state_error,
};

pub(super) struct StartupExecutable<'a> {
    version: &'a StableVersion,
    replacement: Option<&'a UpdateReplacementContext>,
    authority: &'a mut dyn ExternalUpgradeAdoptionAuthority,
}

impl<'a> StartupExecutable<'a> {
    pub(super) const fn new(
        version: &'a StableVersion,
        replacement: Option<&'a UpdateReplacementContext>,
        authority: &'a mut dyn ExternalUpgradeAdoptionAuthority,
    ) -> Self {
        Self {
            version,
            replacement,
            authority,
        }
    }
}

pub(super) fn admit_update_start(
    cache_dir: &Path,
    coordinator: &FileRuntimeCoordinator,
    installation: &Installation,
    exact_resume: Option<SessionId>,
    executable: StartupExecutable<'_>,
    ids: &mut impl IdGenerator,
    now: Timestamp,
) -> Result<StartupAdmission, CliError> {
    let StartupExecutable {
        version,
        replacement,
        authority,
    } = executable;
    admit_with_authority(
        cache_dir,
        coordinator,
        installation,
        exact_resume,
        version,
        replacement,
        ids,
        now,
        STARTUP_ADMISSION_TIMEOUT,
        authority,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "startup admission keeps exact installation, version, and time authority explicit"
)]
fn admit_with_authority(
    cache_dir: &Path,
    coordinator: &FileRuntimeCoordinator,
    installation: &Installation,
    exact_resume: Option<SessionId>,
    current: &StableVersion,
    replacement: Option<&UpdateReplacementContext>,
    ids: &mut impl IdGenerator,
    now: Timestamp,
    wait: Duration,
    authority: &mut dyn ExternalUpgradeAdoptionAuthority,
) -> Result<StartupAdmission, CliError> {
    let state = FileUpdateStateStore::new(cache_dir).map_err(update_state_error)?;
    let cache = state
        .load(installation.identity)
        .map_err(update_state_error)?;
    let Some(observed) = cache.observed_installed_version else {
        return ordinary_admission(&state, installation.identity, exact_resume)
            .map(StartupAdmission::ready);
    };
    match current.relation_to_observed(&observed) {
        InstalledVersionRelation::Equal if cache.external_restart.is_some() => {
            let pending = cache.external_restart.as_ref().ok_or_else(|| {
                update_state_error(UpdateError::State(
                    "external restart authority disappeared".to_owned(),
                ))
            })?;
            if is_exact_replacement(replacement, exact_resume, current, pending)
                || is_acknowledged_manual_resume(exact_resume, pending)
            {
                wait_for_replacement_admission(&state, installation.identity, exact_resume, wait)
                    .map(StartupAdmission::ready)
            } else {
                reconcile_pending_restart(
                    &state,
                    coordinator,
                    installation,
                    exact_resume,
                    current,
                    pending,
                    authority,
                )
            }
        }
        InstalledVersionRelation::Equal => {
            ordinary_admission(&state, installation.identity, exact_resume)
                .map(StartupAdmission::ready)
        }
        InstalledVersionRelation::CurrentOlder => Err(obsolete_error(current, &observed)),
        InstalledVersionRelation::CurrentNewer if cache.external_restart.is_some() => {
            let pending = cache.external_restart.as_ref().ok_or_else(|| {
                update_state_error(UpdateError::State(
                    "external restart authority disappeared".to_owned(),
                ))
            })?;
            Err(pending_target_error(current, &observed, pending))
        }
        InstalledVersionRelation::CurrentNewer => reconcile_newer(
            &state,
            coordinator,
            installation,
            exact_resume,
            &observed,
            current,
            replacement,
            ids,
            now,
            wait,
            authority,
        ),
    }
}

fn ordinary_admission(
    state: &FileUpdateStateStore,
    installation: InstallationIdentity,
    exact_resume: Option<SessionId>,
) -> Result<Box<dyn UpdateLease>, CliError> {
    state
        .try_startup_lock(installation)
        .map_err(update_state_error)?
        .ok_or_else(|| convergence_active(exact_resume))
}

fn wait_for_replacement_admission(
    state: &FileUpdateStateStore,
    installation: InstallationIdentity,
    exact_resume: Option<SessionId>,
    wait: Duration,
) -> Result<Box<dyn UpdateLease>, CliError> {
    state
        .wait_for_startup_lock(installation, wait)
        .map_err(update_state_error)?
        .ok_or_else(|| convergence_active(exact_resume))
}

#[expect(
    clippy::too_many_arguments,
    reason = "external reconciliation keeps the compared observation and authorities explicit"
)]
fn reconcile_newer(
    state: &FileUpdateStateStore,
    coordinator: &FileRuntimeCoordinator,
    installation: &Installation,
    exact_resume: Option<SessionId>,
    expected_observed: &StableVersion,
    current: &StableVersion,
    replacement: Option<&UpdateReplacementContext>,
    ids: &mut impl IdGenerator,
    now: Timestamp,
    wait: Duration,
    authority: &mut dyn ExternalUpgradeAdoptionAuthority,
) -> Result<StartupAdmission, CliError> {
    let convergence = state
        .try_lock(installation.identity, UpdateLockKind::Convergence)
        .map_err(update_state_error)?;
    let Some(convergence) = convergence else {
        return follow_convergence(
            state,
            installation.identity,
            exact_resume,
            current,
            replacement,
            wait,
        );
    };
    let cache = state
        .load(installation.identity)
        .map_err(update_state_error)?;
    let observed = cache.observed_installed_version.as_ref().ok_or_else(|| {
        update_state_error(UpdateError::State(
            "update observation changed during external convergence".to_owned(),
        ))
    })?;
    match current.relation_to_observed(observed) {
        InstalledVersionRelation::Equal if cache.external_restart.is_some() => {
            let pending = cache.external_restart.as_ref().ok_or_else(|| {
                update_state_error(UpdateError::State(
                    "external restart authority disappeared".to_owned(),
                ))
            })?;
            if is_exact_replacement(replacement, exact_resume, current, pending)
                || is_acknowledged_manual_resume(exact_resume, pending)
            {
                drop(convergence);
                wait_for_replacement_admission(state, installation.identity, exact_resume, wait)
                    .map(StartupAdmission::ready)
            } else {
                reconcile_locked_pending_restart(
                    state,
                    coordinator,
                    installation,
                    exact_resume,
                    current,
                    pending,
                    convergence,
                    authority,
                )
            }
        }
        InstalledVersionRelation::Equal => Ok(StartupAdmission::ready(convergence)),
        InstalledVersionRelation::CurrentOlder => Err(obsolete_error(current, observed)),
        InstalledVersionRelation::CurrentNewer if cache.external_restart.is_some() => {
            let pending = cache.external_restart.as_ref().ok_or_else(|| {
                update_state_error(UpdateError::State(
                    "external restart authority disappeared".to_owned(),
                ))
            })?;
            Err(pending_target_error(current, observed, pending))
        }
        InstalledVersionRelation::CurrentNewer => {
            if observed != expected_observed {
                return Err(update_state_error(UpdateError::State(
                    "update observation changed during external convergence".to_owned(),
                )));
            }
            adopt_external_candidate(
                state,
                coordinator,
                installation,
                observed,
                current,
                ids,
                now,
                convergence,
                authority,
            )
        }
    }
}

fn reconcile_pending_restart(
    state: &FileUpdateStateStore,
    coordinator: &FileRuntimeCoordinator,
    installation: &Installation,
    exact_resume: Option<SessionId>,
    current: &StableVersion,
    pending: &ExternalRestartPending,
    authority: &mut dyn ExternalUpgradeAdoptionAuthority,
) -> Result<StartupAdmission, CliError> {
    let convergence = state
        .try_lock(installation.identity, UpdateLockKind::Convergence)
        .map_err(update_state_error)?
        .ok_or_else(|| convergence_active(exact_resume))?;
    let cache = state
        .load(installation.identity)
        .map_err(update_state_error)?;
    let observed = cache.observed_installed_version.as_ref().ok_or_else(|| {
        update_state_error(UpdateError::State(
            "update observation changed during pending restart recovery".to_owned(),
        ))
    })?;
    if cache.external_restart.as_ref() != Some(pending) {
        return Err(update_state_error(UpdateError::State(
            "external restart authority changed during recovery".to_owned(),
        )));
    }
    match current.relation_to_observed(observed) {
        InstalledVersionRelation::Equal if cache.restart_needed => {
            reconcile_locked_pending_restart(
                state,
                coordinator,
                installation,
                exact_resume,
                current,
                pending,
                convergence,
                authority,
            )
        }
        InstalledVersionRelation::Equal => Ok(StartupAdmission::ready(convergence)),
        InstalledVersionRelation::CurrentOlder => Err(obsolete_error(current, observed)),
        InstalledVersionRelation::CurrentNewer => Err(update_state_error(UpdateError::State(
            "update observation changed during pending restart recovery".to_owned(),
        ))),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "pending recovery retains the exact session and convergence authority"
)]
fn reconcile_locked_pending_restart(
    state: &FileUpdateStateStore,
    coordinator: &FileRuntimeCoordinator,
    installation: &Installation,
    exact_resume: Option<SessionId>,
    current: &StableVersion,
    pending: &ExternalRestartPending,
    convergence: Box<dyn UpdateLease>,
    authority: &mut dyn ExternalUpgradeAdoptionAuthority,
) -> Result<StartupAdmission, CliError> {
    if let Some(session_id) = exact_resume
        && pending.expectation_for_session(session_id).is_some()
    {
        let lease = admit_pending_external_resume(
            coordinator,
            installation.identity,
            installation.kind,
            current,
            pending,
            session_id,
            convergence,
            authority,
        )
        .map_err(external_error)?;
        return Ok(StartupAdmission::manual_resume(
            lease,
            state.clone(),
            installation.identity,
            current.clone(),
            pending.clone(),
            session_id,
        ));
    }
    complete_pending_restart(
        state,
        coordinator,
        installation,
        current,
        pending,
        convergence,
        authority,
    )
    .map(StartupAdmission::ready)
}

fn complete_pending_restart(
    state: &FileUpdateStateStore,
    coordinator: &FileRuntimeCoordinator,
    installation: &Installation,
    current: &StableVersion,
    pending: &ExternalRestartPending,
    convergence: Box<dyn UpdateLease>,
    authority: &mut dyn ExternalUpgradeAdoptionAuthority,
) -> Result<Box<dyn UpdateLease>, CliError> {
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    ExternalUpgradeCoordinator::new(state, coordinator, &mut gateway)
        .complete_pending_restart(
            installation.identity,
            installation.kind,
            current,
            pending,
            convergence,
            authority,
        )
        .map_err(external_error)
}

fn is_exact_replacement(
    replacement: Option<&UpdateReplacementContext>,
    exact_resume: Option<SessionId>,
    current: &StableVersion,
    pending: &ExternalRestartPending,
) -> bool {
    if pending.target_version() != current {
        return false;
    }
    replacement.is_some_and(|proof| {
        &proof.target_version == current
            && proof.operation_id == pending.operation_id()
            && pending.expectations().iter().any(|expectation| {
                exact_resume == Some(expectation.session_id())
                    && proof.previous_instance_id == expectation.previous_instance_id()
                    && std::process::id() == expectation.previous_pid()
            })
    })
}

fn is_acknowledged_manual_resume(
    exact_resume: Option<SessionId>,
    pending: &ExternalRestartPending,
) -> bool {
    exact_resume.is_some_and(|session_id| pending.manually_acknowledged(session_id))
}

fn follow_convergence(
    state: &FileUpdateStateStore,
    installation: InstallationIdentity,
    exact_resume: Option<SessionId>,
    current: &StableVersion,
    replacement: Option<&UpdateReplacementContext>,
    wait: Duration,
) -> Result<StartupAdmission, CliError> {
    let admission = state
        .wait_for_startup_lock(installation, wait)
        .map_err(update_state_error)?
        .ok_or_else(|| convergence_active(exact_resume))?;
    let cache = state.load(installation).map_err(update_state_error)?;
    let observed = cache.observed_installed_version.as_ref().ok_or_else(|| {
        update_state_error(UpdateError::State(
            "update observation disappeared during external convergence".to_owned(),
        ))
    })?;
    match current.relation_to_observed(observed) {
        InstalledVersionRelation::Equal
            if cache.external_restart.as_ref().is_some_and(|pending| {
                is_exact_replacement(replacement, exact_resume, current, pending)
                    || is_acknowledged_manual_resume(exact_resume, pending)
            }) =>
        {
            Ok(StartupAdmission::ready(admission))
        }
        InstalledVersionRelation::Equal if cache.external_restart.is_some() => {
            Err(convergence_active(exact_resume))
        }
        InstalledVersionRelation::Equal => Ok(StartupAdmission::ready(admission)),
        InstalledVersionRelation::CurrentOlder => Err(obsolete_error(current, observed)),
        InstalledVersionRelation::CurrentNewer if cache.external_restart.is_some() => {
            let pending = cache.external_restart.as_ref().ok_or_else(|| {
                update_state_error(UpdateError::State(
                    "external restart authority disappeared".to_owned(),
                ))
            })?;
            Err(pending_target_error(current, observed, pending))
        }
        InstalledVersionRelation::CurrentNewer => Err(convergence_active(exact_resume)),
    }
}

#[cfg(test)]
mod tests;
