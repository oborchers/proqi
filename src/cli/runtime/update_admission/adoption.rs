//! Final coordination for one verified newer external executable.

use crate::{
    adapters::{
        control::LocalUpdateControlClient,
        runtime::{FileRuntimeCoordinator, SystemIdGenerator},
        update::FileUpdateStateStore,
    },
    application::{ExternalUpgradeCacheStatus, ExternalUpgradeCoordinator},
    domain::{Installation, StableVersion, Timestamp},
    ports::{
        environment::IdGenerator,
        update::{ExternalUpgradeAdoptionAuthority, UpdateError, UpdateLease},
    },
};

use super::{
    CliError, EXTERNAL_PREPARATION_WINDOW_MILLIS, StartupAdmission, external_error,
    update_state_error,
};

#[expect(
    clippy::too_many_arguments,
    reason = "cache adoption retains every compared value and authority at the transition"
)]
pub(super) fn adopt_external_candidate(
    state: &FileUpdateStateStore,
    coordinator: &FileRuntimeCoordinator,
    installation: &Installation,
    observed: &StableVersion,
    current: &StableVersion,
    ids: &mut impl IdGenerator,
    now: Timestamp,
    convergence: Box<dyn UpdateLease>,
    authority: &mut dyn ExternalUpgradeAdoptionAuthority,
) -> Result<StartupAdmission, CliError> {
    let mut gateway = LocalUpdateControlClient::new(SystemIdGenerator);
    let deadline = Timestamp::from_millis(
        now.as_millis()
            .saturating_add(EXTERNAL_PREPARATION_WINDOW_MILLIS),
    );
    let admission = ExternalUpgradeCoordinator::new(state, coordinator, &mut gateway)
        .reconcile(
            ids.request_id(),
            installation.identity,
            installation.kind,
            observed,
            current,
            deadline,
            convergence,
            &(),
            authority,
        )
        .map_err(external_error)?;
    let (lease, status, adoption) = admission.into_parts();
    match status {
        ExternalUpgradeCacheStatus::DeferredUntilStoreReady => {
            let adoption = adoption.ok_or_else(|| {
                update_state_error(UpdateError::Coordination(
                    "external adoption authority disappeared".to_owned(),
                ))
            })?;
            Ok(StartupAdmission::deferred(
                lease,
                adoption,
                state.clone(),
                installation.identity,
                observed.clone(),
                current.clone(),
            ))
        }
        ExternalUpgradeCacheStatus::Reconciled if adoption.is_none() => {
            Ok(StartupAdmission::ready(lease))
        }
        ExternalUpgradeCacheStatus::Reconciled => {
            Err(update_state_error(UpdateError::Coordination(
                "reconciled external adoption retained schema authority".to_owned(),
            )))
        }
    }
}
