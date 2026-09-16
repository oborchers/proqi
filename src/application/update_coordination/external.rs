//! Safe adoption of a verified installation replaced outside Proqi.

use std::time::Duration;

use thiserror::Error;

use crate::{
    domain::{
        ExternalRestartPending, InstallationIdentity, InstallationKind, InstanceId, RequestId,
        SessionId, StableVersion, Timestamp,
    },
    ports::{
        runtime::{InstanceInfo, Lease},
        update::{
            ExternalCacheTransition, ExternalUpgradeAdoptionAuthority,
            ExternalUpgradeAuthorityError, UpdateCancellation, UpdateError, UpdateInstanceRegistry,
            UpdateLease, UpdateParticipantGateway, UpdatePrepareRequest, UpdateStateStore,
        },
    },
};

use super::{
    REPLACEMENT_TIMEOUT,
    preflight::{preflight, release_all},
    quiescence::quiesce_prepared,
};

mod cache;
mod manual;
mod participants;
mod replacement;

pub use manual::admit_pending_external_resume;

use cache::reconcile_cache;
use participants::{blockers, blockers_by_instance, capacity_exceeded, plan, preparation_blockers};
use replacement::{
    pending_replacement_blockers, pending_restart, replacement_blockers, replacement_expectations,
    restart_all,
};

/// Cache work still required after startup has proved real schema admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalUpgradeCacheStatus {
    /// No registered participant existed, so cache adoption must follow successful store opening.
    DeferredUntilStoreReady,
    /// Exact coordinated replacements were verified and the cache transition is complete.
    Reconciled,
}

/// Convergence lease plus the typed cache status of one external startup.
pub struct ExternalUpgradeAdmission {
    lease: Box<dyn UpdateLease>,
    cache_status: ExternalUpgradeCacheStatus,
    adoption: Option<Box<dyn Lease>>,
}

impl ExternalUpgradeAdmission {
    /// Separate startup ownership from the remaining cache work.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Box<dyn UpdateLease>,
        ExternalUpgradeCacheStatus,
        Option<Box<dyn Lease>>,
    ) {
        (self.lease, self.cache_status, self.adoption)
    }
}

/// Stable reason that one exact live runtime prevents external installation adoption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalUpgradeBlockerReason {
    /// The owner predates the external convergence protocol.
    OlderIncompatible,
    /// The owner advertises state that cannot safely share or release the schema.
    IncompatibleWriter,
    /// A newer owner proves that this executable is obsolete.
    NewerRuntime,
    /// The owner could not enter the reversible preparation barrier.
    PreparationFailed,
    /// The installation cannot replace live owners automatically.
    RestartUnsupported,
    /// The compatible live cohort exceeds the bounded durable replacement protocol.
    CohortCapacityExceeded,
    /// The owner did not prove irreversible schema quiescence.
    QuiescenceUnverified,
    /// The exact session did not become ready under the active executable.
    ReplacementIncomplete,
}

impl ExternalUpgradeBlockerReason {
    /// Stable machine spelling for bounded CLI details.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OlderIncompatible => "older_incompatible",
            Self::IncompatibleWriter => "incompatible_writer",
            Self::NewerRuntime => "newer_runtime",
            Self::PreparationFailed => "preparation_failed",
            Self::RestartUnsupported => "restart_unsupported",
            Self::CohortCapacityExceeded => "cohort_capacity_exceeded",
            Self::QuiescenceUnverified => "quiescence_unverified",
            Self::ReplacementIncomplete => "replacement_incomplete",
        }
    }
}

/// Content-free identity of one authoritative live runtime blocking convergence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalUpgradeBlocker {
    /// Exact process instance observed under its live session lock.
    pub instance_id: InstanceId,
    /// Exact durable session owned by the runtime.
    pub session_id: SessionId,
    /// Canonical application version advertised by that owner, when valid.
    pub version: Option<StableVersion>,
    /// Typed reason this owner could not converge.
    pub reason: ExternalUpgradeBlockerReason,
}

/// External installation adoption failure before schema admission.
#[derive(Debug, Error)]
pub enum ExternalUpgradeFailure {
    /// Exact live writers cannot safely converge.
    #[error("live Proqi sessions block external upgrade convergence")]
    Blocked(Vec<ExternalUpgradeBlocker>),
    /// Too many compatible live sessions exist for one bounded exact replacement cohort.
    #[error(
        "{participant_count} live Proqi sessions exceed the external replacement limit of {maximum}"
    )]
    Capacity {
        /// Bounded exact identities suitable for actionable diagnostics.
        blockers: Vec<ExternalUpgradeBlocker>,
        /// Exact compatible live-session count observed under convergence ownership.
        participant_count: usize,
        /// Maximum durable exact replacement cohort supported by this executable.
        maximum: usize,
    },
    /// Prepared sessions did not all resume under exact replacement identities.
    #[error("external upgrade replacement convergence is incomplete")]
    Incomplete(Vec<ExternalUpgradeBlocker>),
    /// Durable cache reconciliation failed after exact sessions became quiescent.
    #[error("external upgrade cache persistence failed after quiescence: {source}")]
    Persistence {
        /// Exact sessions whose automatic replacement could not be admitted.
        blockers: Vec<ExternalUpgradeBlocker>,
        /// Private atomic state failure that prevented safe admission.
        source: UpdateError,
    },
    /// Final schema or installation authority changed after participants became quiescent.
    #[error("external upgrade authority changed after quiescence: {source}")]
    Authority {
        /// Exact sessions whose reversible recovery was requested.
        blockers: Vec<ExternalUpgradeBlocker>,
        /// Typed proof failure that prevented cache adoption.
        source: ExternalUpgradeAuthorityError,
    },
    /// The final schema or installation proof failed outside a reversible participant cohort.
    #[error(transparent)]
    AuthorityUnavailable(#[from] ExternalUpgradeAuthorityError),
    /// A cache, registry, lock, or control boundary failed.
    #[error(transparent)]
    Update(#[from] UpdateError),
}

/// Reuses update preparation, quiescence, restart, and replacement verification externally.
pub struct ExternalUpgradeCoordinator<'a, S, R, G> {
    state: &'a S,
    registry: &'a R,
    gateway: &'a mut G,
}

impl<'a, S, R, G> ExternalUpgradeCoordinator<'a, S, R, G>
where
    S: UpdateStateStore,
    R: UpdateInstanceRegistry,
    G: UpdateParticipantGateway,
{
    /// Bind the existing update owners used by external convergence.
    #[must_use]
    pub const fn new(state: &'a S, registry: &'a R, gateway: &'a mut G) -> Self {
        Self {
            state,
            registry,
            gateway,
        }
    }

    /// Reconcile one exact stale observation and return startup admission.
    /// # Errors
    /// Refuses incompatible writers, unverified quiescence, incomplete exact replacement,
    /// cancellation, cache conflicts, and coordination failures before schema admission.
    #[expect(
        clippy::too_many_arguments,
        reason = "external convergence keeps exact authority and deadlines explicit"
    )]
    pub fn reconcile(
        &mut self,
        operation_id: RequestId,
        installation: InstallationIdentity,
        installation_kind: InstallationKind,
        observed: &StableVersion,
        current: &StableVersion,
        deadline: Timestamp,
        convergence: Box<dyn UpdateLease>,
        cancellation: &dyn UpdateCancellation,
        authority: &mut dyn ExternalUpgradeAdoptionAuthority,
    ) -> Result<ExternalUpgradeAdmission, ExternalUpgradeFailure> {
        authority.establish()?;
        let plan = plan(
            self.registry.active_instances()?,
            installation,
            installation_kind,
            current,
        );
        if !plan.blockers.is_empty() {
            return Err(ExternalUpgradeFailure::Blocked(plan.blockers));
        }
        if let Some(capacity) = capacity_exceeded(&plan.compatible_older) {
            return Err(ExternalUpgradeFailure::Capacity {
                blockers: capacity.blockers,
                participant_count: capacity.participant_count,
                maximum: capacity.maximum,
            });
        }
        if plan.compatible_older.is_empty() {
            let adoption = authority.acquire()?;
            return Ok(ExternalUpgradeAdmission {
                lease: convergence,
                cache_status: ExternalUpgradeCacheStatus::DeferredUntilStoreReady,
                adoption: Some(adoption),
            });
        }
        if cancellation.is_cancelled() {
            return Err(ExternalUpgradeFailure::Update(UpdateError::Coordination(
                "external convergence cancelled before preparation".to_owned(),
            )));
        }
        let prepare = UpdatePrepareRequest {
            operation_id,
            target_version: current.clone(),
            installation_identity: installation,
            deadline,
        };
        let ready = match preflight(self.gateway, &plan.compatible_older, &prepare) {
            Ok(ready) => ready,
            Err(failure) => {
                release_all(self.gateway, &failure.ready, operation_id);
                return Err(ExternalUpgradeFailure::Blocked(preparation_blockers(
                    &plan.compatible_older,
                    failure.blocker,
                )));
            }
        };
        let pending = match pending_restart(operation_id, current, &ready) {
            Ok(pending) => pending,
            Err(error) => {
                release_all(self.gateway, &ready, operation_id);
                return Err(error);
            }
        };
        let quiescence = quiesce_prepared(self.gateway, &ready, operation_id, current);
        if !quiescence.failed.is_empty() {
            return self.recover_partial_quiescence(
                operation_id,
                current,
                convergence,
                &ready,
                &quiescence.quiesced,
                &quiescence.failed,
            );
        }
        self.finish_quiesced(
            operation_id,
            installation,
            observed,
            current,
            convergence,
            &quiescence.quiesced,
            &pending,
            authority,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "exact external convergence authority and its durable pending cohort stay explicit"
    )]
    fn finish_quiesced(
        &mut self,
        operation_id: RequestId,
        installation: InstallationIdentity,
        observed: &StableVersion,
        current: &StableVersion,
        convergence: Box<dyn UpdateLease>,
        quiesced: &[InstanceInfo],
        pending: &ExternalRestartPending,
        authority: &mut dyn ExternalUpgradeAdoptionAuthority,
    ) -> Result<ExternalUpgradeAdmission, ExternalUpgradeFailure> {
        let adoption = match authority.acquire() {
            Ok(adoption) => adoption,
            Err(source) => {
                return self.recover_quiesced_after_authority_failure(
                    operation_id,
                    current,
                    convergence,
                    quiesced,
                    source,
                );
            }
        };
        match reconcile_cache(self.state, installation, observed, current, Some(pending)) {
            Ok(()) => {}
            Err(ExternalUpgradeFailure::Update(source)) => {
                drop(adoption);
                return self.recover_quiesced_after_cache_failure(
                    operation_id,
                    current,
                    convergence,
                    quiesced,
                    source,
                );
            }
            Err(other) => {
                drop(adoption);
                return Err(other);
            }
        }
        drop(adoption);
        drop(convergence);
        let restart = restart_all(self.gateway, quiesced, operation_id, current);
        let expected = replacement_expectations(pending);
        let missing = self.registry.wait_for_replacements(
            installation,
            current,
            &expected,
            REPLACEMENT_TIMEOUT,
            &(),
        )?;
        let incomplete = replacement_blockers(quiesced, &restart.failed, &missing);
        if !incomplete.is_empty() {
            return Err(ExternalUpgradeFailure::Incomplete(incomplete));
        }
        let admission = self
            .state
            .wait_for_startup_lock(installation, REPLACEMENT_TIMEOUT)?
            .ok_or_else(|| {
                ExternalUpgradeFailure::Update(UpdateError::Coordination(
                    "external replacement startup admission timed out".to_owned(),
                ))
            })?;
        authority.revalidate_installation()?;
        match self
            .state
            .complete_external_restart(installation, current, pending)?
        {
            ExternalCacheTransition::Applied | ExternalCacheTransition::AlreadyApplied => {
                Ok(ExternalUpgradeAdmission {
                    lease: admission,
                    cache_status: ExternalUpgradeCacheStatus::Reconciled,
                    adoption: None,
                })
            }
            ExternalCacheTransition::Conflict => {
                Err(ExternalUpgradeFailure::Update(UpdateError::State(
                    "update cache changed before external restart completion".to_owned(),
                )))
            }
        }
    }

    /// Clear one exact pending restart after authoritative live-owner revalidation.
    /// # Errors
    /// Refuses every older, newer, malformed, or storage-incompatible live runtime and cache
    /// conflicts. The caller retains exclusive convergence ownership through the transition.
    pub fn complete_pending_restart(
        &self,
        installation: InstallationIdentity,
        installation_kind: InstallationKind,
        current: &StableVersion,
        pending: &ExternalRestartPending,
        convergence: Box<dyn UpdateLease>,
        authority: &mut dyn ExternalUpgradeAdoptionAuthority,
    ) -> Result<Box<dyn UpdateLease>, ExternalUpgradeFailure> {
        if pending.target_version() != current {
            return Err(ExternalUpgradeFailure::Update(UpdateError::State(
                "pending external restart target changed".to_owned(),
            )));
        }
        authority.establish()?;
        let plan = plan(
            self.registry.active_instances()?,
            installation,
            installation_kind,
            current,
        );
        let mut found = plan.blockers;
        found.extend(blockers(
            &plan.compatible_older,
            ExternalUpgradeBlockerReason::ReplacementIncomplete,
        ));
        let expected = replacement_expectations(pending);
        let missing = self.registry.wait_for_replacements(
            installation,
            current,
            &expected,
            Duration::ZERO,
            &(),
        )?;
        found.extend(pending_replacement_blockers(pending, &missing));
        if !found.is_empty() {
            return Err(ExternalUpgradeFailure::Incomplete(found));
        }
        authority.revalidate_installation()?;
        match self
            .state
            .complete_external_restart(installation, current, pending)?
        {
            ExternalCacheTransition::Applied | ExternalCacheTransition::AlreadyApplied => {
                Ok(convergence)
            }
            ExternalCacheTransition::Conflict => {
                Err(ExternalUpgradeFailure::Update(UpdateError::State(
                    "update cache changed before pending restart completion".to_owned(),
                )))
            }
        }
    }

    fn recover_partial_quiescence(
        &mut self,
        operation_id: RequestId,
        current: &StableVersion,
        convergence: Box<dyn UpdateLease>,
        prepared: &[InstanceInfo],
        quiesced: &[InstanceInfo],
        failed: &[InstanceId],
    ) -> Result<ExternalUpgradeAdmission, ExternalUpgradeFailure> {
        let mut affected = blockers_by_instance(
            prepared,
            failed,
            ExternalUpgradeBlockerReason::QuiescenceUnverified,
        );
        if quiesced.is_empty() {
            return Err(ExternalUpgradeFailure::Blocked(affected));
        }
        drop(convergence);
        let _restart = restart_all(self.gateway, quiesced, operation_id, current);
        affected.extend(blockers(
            quiesced,
            ExternalUpgradeBlockerReason::ReplacementIncomplete,
        ));
        Err(ExternalUpgradeFailure::Incomplete(affected))
    }

    fn recover_quiesced_after_cache_failure(
        &mut self,
        operation_id: RequestId,
        current: &StableVersion,
        convergence: Box<dyn UpdateLease>,
        quiesced: &[InstanceInfo],
        source: UpdateError,
    ) -> Result<ExternalUpgradeAdmission, ExternalUpgradeFailure> {
        drop(convergence);
        let _restart = restart_all(self.gateway, quiesced, operation_id, current);
        Err(ExternalUpgradeFailure::Persistence {
            blockers: blockers(
                quiesced,
                ExternalUpgradeBlockerReason::ReplacementIncomplete,
            ),
            source,
        })
    }

    fn recover_quiesced_after_authority_failure(
        &mut self,
        operation_id: RequestId,
        current: &StableVersion,
        convergence: Box<dyn UpdateLease>,
        quiesced: &[InstanceInfo],
        source: ExternalUpgradeAuthorityError,
    ) -> Result<ExternalUpgradeAdmission, ExternalUpgradeFailure> {
        drop(convergence);
        let _restart = restart_all(self.gateway, quiesced, operation_id, current);
        Err(ExternalUpgradeFailure::Authority {
            blockers: blockers(
                quiesced,
                ExternalUpgradeBlockerReason::ReplacementIncomplete,
            ),
            source,
        })
    }
}

#[cfg(test)]
mod tests;
