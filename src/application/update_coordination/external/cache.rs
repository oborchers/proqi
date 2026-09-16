//! Atomic external-convergence cache transition.

use crate::{
    domain::{ExternalRestartPending, InstallationIdentity, StableVersion},
    ports::update::{ExternalCacheTransition, UpdateError, UpdateStateStore},
};

use super::ExternalUpgradeFailure;

pub(super) fn reconcile_cache<S: UpdateStateStore>(
    state: &S,
    installation: InstallationIdentity,
    observed: &StableVersion,
    current: &StableVersion,
    pending: Option<&ExternalRestartPending>,
) -> Result<(), ExternalUpgradeFailure> {
    match state.reconcile_external_upgrade(installation, observed, current, pending)? {
        ExternalCacheTransition::Applied | ExternalCacheTransition::AlreadyApplied => Ok(()),
        ExternalCacheTransition::Conflict => Err(ExternalUpgradeFailure::Update(
            UpdateError::State("update cache changed during external convergence".to_owned()),
        )),
    }
}
