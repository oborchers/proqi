//! Safe admission for one exact manual resume after automatic replacement failed.

use crate::{
    domain::{
        ExternalRestartPending, InstallationIdentity, InstallationKind, SessionId, StableVersion,
    },
    ports::update::{
        ExternalUpgradeAdoptionAuthority, UpdateError, UpdateInstanceRegistry, UpdateLease,
    },
};

use super::{
    ExternalUpgradeBlockerReason, ExternalUpgradeFailure,
    participants::{blocker, blockers, plan},
};

/// Admit one explicitly requested pending session while preserving every unfinished peer.
///
/// The caller must retain the convergence lease until the exact session lease is acquired and the
/// cache acknowledges that manual recovery.
///
/// # Errors
///
/// Refuses nonmembers, target drift, any incompatible live writer, or an already live owner for
/// the requested session.
#[expect(
    clippy::too_many_arguments,
    reason = "manual recovery keeps the exact pending session and both authorities explicit"
)]
pub fn admit_pending_external_resume<R: UpdateInstanceRegistry>(
    registry: &R,
    installation: InstallationIdentity,
    installation_kind: InstallationKind,
    current: &StableVersion,
    pending: &ExternalRestartPending,
    session_id: SessionId,
    convergence: Box<dyn UpdateLease>,
    authority: &mut dyn ExternalUpgradeAdoptionAuthority,
) -> Result<Box<dyn UpdateLease>, ExternalUpgradeFailure> {
    if pending.target_version() != current || pending.expectation_for_session(session_id).is_none()
    {
        return Err(ExternalUpgradeFailure::Update(UpdateError::State(
            "manual external recovery does not match the pending restart cohort".to_owned(),
        )));
    }
    authority.establish()?;
    let instances = registry.active_instances()?;
    let live_target = instances
        .iter()
        .find(|instance| instance.session_id == session_id)
        .map(|instance| {
            blocker(
                instance,
                ExternalUpgradeBlockerReason::ReplacementIncomplete,
            )
        });
    let plan = plan(instances, installation, installation_kind, current);
    let mut found = plan.blockers;
    found.extend(blockers(
        &plan.compatible_older,
        ExternalUpgradeBlockerReason::ReplacementIncomplete,
    ));
    found.extend(live_target);
    if !found.is_empty() {
        return Err(ExternalUpgradeFailure::Incomplete(found));
    }
    authority.revalidate_installation()?;
    Ok(convergence)
}
