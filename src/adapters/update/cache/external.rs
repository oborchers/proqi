//! Exact cache transitions for externally installed upgrades.

use crate::{
    domain::{ExternalRestartPending, InstallationIdentity, SessionId, StableVersion},
    ports::update::{ExternalCacheTransition, UpdateError},
};

use super::FileUpdateStateStore;

pub(super) fn reconcile(
    store: &FileUpdateStateStore,
    installation: InstallationIdentity,
    expected_observed: &StableVersion,
    installed: &StableVersion,
    pending: Option<&ExternalRestartPending>,
) -> Result<ExternalCacheTransition, UpdateError> {
    if pending.is_some_and(|value| value.target_version() != installed) {
        return Err(UpdateError::State(
            "external restart target does not match installed version".to_owned(),
        ));
    }
    store.transition(installation, |state| {
        if state.observed_installed_version.as_ref() == Some(installed)
            && state.restart_needed == pending.is_some()
            && state.external_restart.as_ref() == pending
        {
            Ok((ExternalCacheTransition::AlreadyApplied, false))
        } else if state.observed_installed_version.as_ref() == Some(expected_observed)
            && installed > expected_observed
        {
            state.observed_installed_version = Some(installed.clone());
            state.restart_needed = pending.is_some();
            state.external_restart = pending.cloned();
            Ok((ExternalCacheTransition::Applied, true))
        } else {
            Ok((ExternalCacheTransition::Conflict, false))
        }
    })
}

pub(super) fn complete(
    store: &FileUpdateStateStore,
    installation: InstallationIdentity,
    installed: &StableVersion,
    pending: &ExternalRestartPending,
) -> Result<ExternalCacheTransition, UpdateError> {
    store.transition(installation, |state| {
        if pending.target_version() != installed
            || state.observed_installed_version.as_ref() != Some(installed)
        {
            return Ok((ExternalCacheTransition::Conflict, false));
        }
        if state.restart_needed && state.external_restart.as_ref() == Some(pending) {
            state.restart_needed = false;
            state.external_restart = None;
            Ok((ExternalCacheTransition::Applied, true))
        } else if !state.restart_needed && state.external_restart.is_none() {
            Ok((ExternalCacheTransition::AlreadyApplied, false))
        } else {
            Ok((ExternalCacheTransition::Conflict, false))
        }
    })
}

pub(super) fn acknowledge_resume(
    store: &FileUpdateStateStore,
    installation: InstallationIdentity,
    installed: &StableVersion,
    pending: &ExternalRestartPending,
    session_id: SessionId,
) -> Result<ExternalCacheTransition, UpdateError> {
    if pending.target_version() != installed
        || pending.expectation_for_session(session_id).is_none()
    {
        return Ok(ExternalCacheTransition::Conflict);
    }
    let after = pending
        .after_manual_resume(session_id)
        .map_err(|error| UpdateError::State(error.to_string()))?;
    store.transition(installation, |state| {
        if state.observed_installed_version.as_ref() != Some(installed) {
            return Ok((ExternalCacheTransition::Conflict, false));
        }
        if state.restart_needed == after.is_some()
            && state.external_restart.as_ref() == after.as_ref()
        {
            return Ok((ExternalCacheTransition::AlreadyApplied, false));
        }
        if !state.restart_needed || state.external_restart.as_ref() != Some(pending) {
            return Ok((ExternalCacheTransition::Conflict, false));
        }
        state.external_restart = after;
        state.restart_needed = state.external_restart.is_some();
        Ok((ExternalCacheTransition::Applied, true))
    })
}
