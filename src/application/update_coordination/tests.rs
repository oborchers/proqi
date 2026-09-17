use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::{
    application::test_support::{TestClock, TestIds},
    domain::{
        ExternalRestartExpectation, ExternalRestartPending, InstallationIdentity, InstanceId,
        ReleaseHighlightAnnouncement, RequestId, SessionId, StableVersion, Timestamp,
        UpdateCacheState,
    },
    ports::{
        control::CONTROL_PROTOCOL_VERSION,
        environment::IdGenerator as _,
        runtime::{InstanceInfo, UpdateInstanceContext},
        store::STORAGE_PROTOCOL_VERSION,
        update::{
            ExternalCacheTransition, ReleaseObservation, RestartCompletion,
            UPDATE_CONTROL_PROTOCOL_VERSION, UpdateCancellation, UpdateError, UpdateInstaller,
            UpdateInstanceRegistry, UpdateLease, UpdateLockKind, UpdateParticipantGateway,
            UpdatePrepareReply, UpdatePrepareRequest, UpdateQuiesceReply, UpdateQuiesceRequest,
            UpdateReplacementExpectation, UpdateRestartReply, UpdateRestartRequest,
            UpdateStateStore,
        },
    },
};

use super::{UpdateExecutionStatus, UpdateRestartCoordinator};

fn clock() -> TestClock {
    TestClock(Timestamp::from_millis(1_800_000_000_000))
}

mod additional;
mod deadlines;
mod execution;

pub(super) struct Lease(pub(super) Option<Arc<AtomicBool>>);
impl UpdateLease for Lease {}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(owned) = &self.0 {
            owned.store(false, Ordering::Release);
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    installer_owned: Arc<AtomicBool>,
    cache: RefCell<UpdateCacheState>,
    restart_writes: RefCell<Vec<bool>>,
    fail_release_highlights: bool,
    pub(super) fail_external_reconcile: bool,
    pub(super) fail_external_completion: bool,
}

impl UpdateStateStore for State {
    fn load(&self, _: InstallationIdentity) -> Result<UpdateCacheState, UpdateError> {
        Ok(self.cache.borrow().clone())
    }

    fn try_lock(
        &self,
        _: InstallationIdentity,
        kind: UpdateLockKind,
    ) -> Result<Option<Box<dyn UpdateLease>>, UpdateError> {
        if kind != UpdateLockKind::Installer {
            return Ok(Some(Box::new(Lease(None))));
        }
        self.installer_owned
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| {
                Some(Box::new(Lease(Some(Arc::clone(&self.installer_owned))))
                    as Box<dyn UpdateLease>)
            })
            .or(Ok(None))
    }

    fn begin_refresh(
        &self,
        _: InstallationIdentity,
        _: Option<u64>,
    ) -> Result<Option<UpdateCacheState>, UpdateError> {
        let mut cache = self.cache.borrow_mut();
        cache.refresh_generation = cache.refresh_generation.saturating_add(1);
        Ok(Some(cache.clone()))
    }

    fn record_success(
        &self,
        _: InstallationIdentity,
        _: ReleaseObservation,
        _: StableVersion,
        _: Timestamp,
    ) -> Result<UpdateCacheState, UpdateError> {
        Ok(self.cache.borrow().clone())
    }

    fn dismiss(
        &self,
        _: InstallationIdentity,
        version: StableVersion,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.cache.borrow_mut().dismissed_version = Some(version);
        Ok(self.cache.borrow().clone())
    }

    fn skip(
        &self,
        _: InstallationIdentity,
        version: StableVersion,
    ) -> Result<UpdateCacheState, UpdateError> {
        self.cache.borrow_mut().skipped_version = Some(version);
        Ok(self.cache.borrow().clone())
    }

    fn record_restart_state(
        &self,
        _: InstallationIdentity,
        installed: StableVersion,
        restart_needed: bool,
    ) -> Result<UpdateCacheState, UpdateError> {
        if self.cache.borrow().external_restart.is_some() {
            return Err(UpdateError::State(
                "in-app restart state conflicts with pending external restart".to_owned(),
            ));
        }
        self.restart_writes.borrow_mut().push(restart_needed);
        let mut cache = self.cache.borrow_mut();
        cache.observed_installed_version = Some(installed);
        cache.restart_needed = restart_needed;
        cache.external_restart = None;
        Ok(cache.clone())
    }

    fn reconcile_external_upgrade(
        &self,
        _: InstallationIdentity,
        expected_observed: &StableVersion,
        installed: &StableVersion,
        pending: Option<&ExternalRestartPending>,
    ) -> Result<ExternalCacheTransition, UpdateError> {
        if self.fail_external_reconcile {
            return Err(UpdateError::State(
                "injected external reconciliation failure".to_owned(),
            ));
        }
        let mut cache = self.cache.borrow_mut();
        if cache.observed_installed_version.as_ref() == Some(installed)
            && cache.restart_needed == pending.is_some()
            && cache.external_restart.as_ref() == pending
        {
            return Ok(ExternalCacheTransition::AlreadyApplied);
        }
        if cache.observed_installed_version.as_ref() != Some(expected_observed)
            || installed <= expected_observed
        {
            return Ok(ExternalCacheTransition::Conflict);
        }
        cache.observed_installed_version = Some(installed.clone());
        cache.restart_needed = pending.is_some();
        cache.external_restart = pending.cloned();
        Ok(ExternalCacheTransition::Applied)
    }

    fn complete_external_restart(
        &self,
        _: InstallationIdentity,
        installed: &StableVersion,
        pending: &ExternalRestartPending,
    ) -> Result<ExternalCacheTransition, UpdateError> {
        if self.fail_external_completion {
            return Err(UpdateError::State(
                "injected external completion failure".to_owned(),
            ));
        }
        let mut cache = self.cache.borrow_mut();
        if cache.observed_installed_version.as_ref() != Some(installed)
            || pending.target_version() != installed
        {
            return Ok(ExternalCacheTransition::Conflict);
        }
        if !cache.restart_needed && cache.external_restart.is_none() {
            return Ok(ExternalCacheTransition::AlreadyApplied);
        }
        if cache.external_restart.as_ref() != Some(pending) {
            return Ok(ExternalCacheTransition::Conflict);
        }
        cache.restart_needed = false;
        cache.external_restart = None;
        Ok(ExternalCacheTransition::Applied)
    }

    fn acknowledge_external_resume(
        &self,
        _: InstallationIdentity,
        installed: &StableVersion,
        pending: &ExternalRestartPending,
        session_id: SessionId,
    ) -> Result<ExternalCacheTransition, UpdateError> {
        let mut cache = self.cache.borrow_mut();
        let after = pending
            .after_manual_resume(session_id)
            .map_err(|error| UpdateError::State(error.to_string()))?;
        if cache.observed_installed_version.as_ref() != Some(installed) {
            return Ok(ExternalCacheTransition::Conflict);
        }
        if cache.restart_needed == after.is_some()
            && cache.external_restart.as_ref() == after.as_ref()
        {
            return Ok(ExternalCacheTransition::AlreadyApplied);
        }
        if !cache.restart_needed || cache.external_restart.as_ref() != Some(pending) {
            return Ok(ExternalCacheTransition::Conflict);
        }
        cache.external_restart = after;
        cache.restart_needed = cache.external_restart.is_some();
        Ok(ExternalCacheTransition::Applied)
    }

    fn complete_restart(
        &self,
        _: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<RestartCompletion, UpdateError> {
        let mut cache = self.cache.borrow_mut();
        let matches = cache.observed_installed_version.as_ref()
            == Some(announcement.target_version())
            && cache.release_highlights.as_ref().is_some_and(|current| {
                !current.acknowledged() && current.same_upgrade(announcement)
            });
        if matches && cache.restart_needed {
            cache.restart_needed = false;
            return Ok(RestartCompletion::Completed);
        }
        if matches {
            return Ok(RestartCompletion::AlreadyComplete);
        }
        Ok(RestartCompletion::Mismatch)
    }

    fn record_release_highlights(
        &self,
        _: InstallationIdentity,
        announcement: ReleaseHighlightAnnouncement,
    ) -> Result<UpdateCacheState, UpdateError> {
        if self.fail_release_highlights {
            return Err(UpdateError::State(
                "injected highlight write failure".to_owned(),
            ));
        }
        self.cache.borrow_mut().release_highlights = Some(announcement);
        Ok(self.cache.borrow().clone())
    }

    fn discard_release_highlights(
        &self,
        _: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<bool, UpdateError> {
        let mut cache = self.cache.borrow_mut();
        let matches = cache
            .release_highlights
            .as_ref()
            .is_some_and(|current| !current.acknowledged() && current.same_upgrade(announcement));
        if matches {
            cache.release_highlights = None;
        }
        Ok(matches)
    }

    fn acknowledge_release_highlights(
        &self,
        _: InstallationIdentity,
        announcement: &ReleaseHighlightAnnouncement,
    ) -> Result<bool, UpdateError> {
        let mut cache = self.cache.borrow_mut();
        let Some(current) = cache.release_highlights.as_mut() else {
            return Ok(false);
        };
        if current.acknowledged() || !current.same_upgrade(announcement) {
            return Ok(false);
        }
        current.acknowledge();
        Ok(true)
    }
}

pub(super) struct Registry {
    scans: RefCell<VecDeque<Vec<InstanceInfo>>>,
    pub(super) replacement_failures: RefCell<Vec<InstanceId>>,
    fail_replacement_wait: bool,
}

impl UpdateInstanceRegistry for Registry {
    fn active_instances(&self) -> Result<Vec<InstanceInfo>, UpdateError> {
        self.scans
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| UpdateError::Coordination("no fake scan queued".to_owned()))
    }

    fn wait_for_replacements(
        &self,
        _: InstallationIdentity,
        _: &StableVersion,
        _: &[UpdateReplacementExpectation],
        _: Duration,
        _: &dyn UpdateCancellation,
    ) -> Result<Vec<InstanceId>, UpdateError> {
        if self.fail_replacement_wait {
            return Err(UpdateError::Coordination(
                "injected replacement scan failure".to_owned(),
            ));
        }
        Ok(self.replacement_failures.borrow().clone())
    }
}

#[derive(Default)]
pub(super) struct Gateway {
    pub(super) prepared: Vec<InstanceId>,
    prepare_deadlines: Vec<Timestamp>,
    pub(super) released: Vec<InstanceId>,
    pub(super) quiesced: Vec<InstanceId>,
    pub(super) restarted: Vec<InstanceId>,
    block_at: Option<usize>,
    pub(super) fail_prepare_at: Option<usize>,
    pub(super) fail_quiesce: Option<InstanceId>,
    pub(super) fail_restart: Option<InstanceId>,
    pub(super) cancel_after_quiesce: Option<Arc<AtomicBool>>,
}

impl UpdateParticipantGateway for Gateway {
    fn prepare(
        &mut self,
        participant: &InstanceInfo,
        request: &UpdatePrepareRequest,
    ) -> Result<UpdatePrepareReply, UpdateError> {
        let index = self.prepared.len();
        self.prepared.push(participant.instance_id);
        self.prepare_deadlines.push(request.deadline);
        if self.fail_prepare_at == Some(index) {
            return Err(UpdateError::Coordination(
                "participant unavailable".to_owned(),
            ));
        }
        if self.block_at == Some(index) {
            Ok(UpdatePrepareReply::Blocked {
                instance_id: participant.instance_id,
                code: "save_failed".to_owned(),
            })
        } else {
            Ok(UpdatePrepareReply::Ready {
                instance_id: participant.instance_id,
                session_id: participant.session_id,
            })
        }
    }

    fn release(&mut self, participant: &InstanceInfo, _: RequestId) -> Result<(), UpdateError> {
        self.released.push(participant.instance_id);
        Ok(())
    }

    fn quiesce(
        &mut self,
        participant: &InstanceInfo,
        _: &UpdateQuiesceRequest,
    ) -> Result<UpdateQuiesceReply, UpdateError> {
        self.quiesced.push(participant.instance_id);
        if let Some(cancellation) = &self.cancel_after_quiesce {
            cancellation.store(true, Ordering::Release);
        }
        if self.fail_quiesce == Some(participant.instance_id) {
            return Err(UpdateError::Coordination(
                "participant quiescence unavailable".to_owned(),
            ));
        }
        Ok(UpdateQuiesceReply {
            instance_id: participant.instance_id,
            session_id: participant.session_id,
        })
    }

    fn restart(
        &mut self,
        participant: &InstanceInfo,
        _: &UpdateRestartRequest,
    ) -> Result<UpdateRestartReply, UpdateError> {
        self.restarted.push(participant.instance_id);
        Ok(UpdateRestartReply {
            instance_id: participant.instance_id,
            accepted: self.fail_restart != Some(participant.instance_id),
        })
    }
}

struct Installer {
    calls: usize,
    result: Result<StableVersion, UpdateError>,
}

impl UpdateInstaller for Installer {
    fn upgrade(&mut self, _: &StableVersion) -> Result<StableVersion, UpdateError> {
        self.calls += 1;
        self.result.clone()
    }
}

pub(super) fn registry(before: Vec<InstanceInfo>, after: Vec<InstanceInfo>) -> Registry {
    Registry {
        scans: RefCell::new(VecDeque::from([before, after])),
        replacement_failures: RefCell::new(Vec::new()),
        fail_replacement_wait: false,
    }
}

pub(super) fn participants(
    ids: &mut TestIds,
    identity: InstallationIdentity,
    count: usize,
) -> Vec<InstanceInfo> {
    (0..count)
        .map(|_| InstanceInfo {
            instance_id: ids.instance_id(),
            session_id: ids.session_id(),
            pid: 1234,
            version: "0.1.0".to_owned(),
            storage_protocol: STORAGE_PROTOCOL_VERSION,
            control_protocol: Some(CONTROL_PROTOCOL_VERSION),
            control_endpoint: Some("/private/update.sock".to_owned()),
            update: Some(UpdateInstanceContext {
                installation_identity: identity,
                protocol: UPDATE_CONTROL_PROTOCOL_VERSION,
                replacement: None,
            }),
            launch_directory: "/workspace".to_owned(),
            started_at: Timestamp::from_millis(1_800_000_000_000),
        })
        .collect()
}

#[test]
fn update_participants_accept_supported_control_versions_and_reject_both_boundaries() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([39; 32]);
    let mut participant = participants(&mut ids, identity, 1).remove(0);
    participant.control_protocol = Some(CONTROL_PROTOCOL_VERSION - 1);
    assert!(super::is_compatible_update_participant(
        &participant,
        identity
    ));
    participant.control_protocol =
        Some(crate::ports::control::UPDATE_MUTATION_MINIMUM_PROTOCOL - 1);
    assert!(!super::is_compatible_update_participant(
        &participant,
        identity
    ));
    participant.control_protocol = Some(CONTROL_PROTOCOL_VERSION + 1);
    assert!(!super::is_compatible_update_participant(
        &participant,
        identity
    ));
}

fn successful_installer() -> Installer {
    Installer {
        calls: 0,
        result: Ok(version("0.2.0")),
    }
}

pub(super) fn version(value: &str) -> StableVersion {
    StableVersion::parse(value).expect("stable version")
}
