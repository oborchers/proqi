//! Startup ownership whose cache adoption follows successful real-store admission.

use crate::{
    adapters::update::FileUpdateStateStore,
    domain::{ExternalRestartPending, InstallationIdentity, SessionId, StableVersion},
    ports::{
        runtime::Lease,
        update::{ExternalCacheTransition, UpdateError, UpdateLease, UpdateStateStore as _},
    },
};

use super::{CliError, update_state_error};

pub(in crate::cli::runtime) struct StartupAdmission {
    lease: Option<Box<dyn UpdateLease>>,
    adoption: Option<Box<dyn Lease>>,
    deferred: Option<DeferredExternalCache>,
    manual_resume: Option<DeferredManualResume>,
}

struct DeferredExternalCache {
    state: FileUpdateStateStore,
    installation: InstallationIdentity,
    observed: StableVersion,
    current: StableVersion,
}

struct DeferredManualResume {
    state: FileUpdateStateStore,
    installation: InstallationIdentity,
    current: StableVersion,
    pending: ExternalRestartPending,
    session_id: SessionId,
}

impl StartupAdmission {
    pub(super) fn ready(lease: Box<dyn UpdateLease>) -> Self {
        Self {
            lease: Some(lease),
            adoption: None,
            deferred: None,
            manual_resume: None,
        }
    }

    pub(super) fn deferred(
        lease: Box<dyn UpdateLease>,
        adoption: Box<dyn Lease>,
        state: FileUpdateStateStore,
        installation: InstallationIdentity,
        observed: StableVersion,
        current: StableVersion,
    ) -> Self {
        Self {
            lease: Some(lease),
            adoption: Some(adoption),
            deferred: Some(DeferredExternalCache {
                state,
                installation,
                observed,
                current,
            }),
            manual_resume: None,
        }
    }

    pub(super) fn manual_resume(
        lease: Box<dyn UpdateLease>,
        state: FileUpdateStateStore,
        installation: InstallationIdentity,
        current: StableVersion,
        pending: ExternalRestartPending,
        session_id: SessionId,
    ) -> Self {
        Self {
            lease: Some(lease),
            adoption: None,
            deferred: None,
            manual_resume: Some(DeferredManualResume {
                state,
                installation,
                current,
                pending,
                session_id,
            }),
        }
    }

    pub(in crate::cli::runtime) fn requires_exclusive_store(&self) -> bool {
        self.adoption.is_some()
    }

    pub(in crate::cli::runtime) fn finish_after_store_ready(&mut self) -> Result<(), CliError> {
        let Some(deferred) = self.deferred.take() else {
            return Ok(());
        };
        let result = match deferred.state.reconcile_external_upgrade(
            deferred.installation,
            &deferred.observed,
            &deferred.current,
            None,
        ) {
            Ok(ExternalCacheTransition::Applied | ExternalCacheTransition::AlreadyApplied) => {
                Ok(())
            }
            Ok(ExternalCacheTransition::Conflict) => Err(update_state_error(UpdateError::State(
                "update cache changed after external schema admission".to_owned(),
            ))),
            Err(error) => Err(update_state_error(error)),
        };
        if result.is_ok() {
            drop(self.adoption.take());
        }
        result
    }

    pub(in crate::cli::runtime) fn finish_exact_resume(
        &mut self,
        session_id: SessionId,
    ) -> Result<(), CliError> {
        let Some(deferred) = self.manual_resume.as_ref() else {
            return Ok(());
        };
        if deferred.session_id != session_id {
            return Err(update_state_error(UpdateError::State(
                "opened session does not match pending external recovery".to_owned(),
            )));
        }
        let transition = deferred
            .state
            .acknowledge_external_resume(
                deferred.installation,
                &deferred.current,
                &deferred.pending,
                session_id,
            )
            .map_err(update_state_error)?;
        match transition {
            ExternalCacheTransition::Applied | ExternalCacheTransition::AlreadyApplied => {
                self.manual_resume = None;
                Ok(())
            }
            ExternalCacheTransition::Conflict => Err(update_state_error(UpdateError::State(
                "pending external recovery changed before exact resume acknowledgement".to_owned(),
            ))),
        }
    }

    pub(in crate::cli::runtime) fn into_lease(mut self) -> Option<Box<dyn UpdateLease>> {
        self.lease.take()
    }
}
