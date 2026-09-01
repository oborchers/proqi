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
    application::test_support::TestIds,
    domain::{
        InstallationIdentity, InstanceId, ReleaseHighlightAnnouncement, RequestId, StableVersion,
        Timestamp, UpdateCacheState,
    },
    ports::{
        control::CONTROL_PROTOCOL_VERSION,
        environment::IdGenerator as _,
        runtime::{InstanceInfo, UpdateInstanceContext},
        store::STORAGE_PROTOCOL_VERSION,
        update::{
            HomebrewInstaller, ReleaseObservation, UPDATE_CONTROL_PROTOCOL_VERSION,
            UpdateCancellation, UpdateError, UpdateInstanceRegistry, UpdateLease, UpdateLockKind,
            UpdateParticipantGateway, UpdatePrepareReply, UpdatePrepareRequest,
            UpdateReplacementExpectation, UpdateRestartReply, UpdateRestartRequest,
            UpdateStateStore,
        },
    },
};

use super::{UpdateExecutionStatus, UpdateRestartCoordinator};

mod additional;

struct Lease(Option<Arc<AtomicBool>>);
impl UpdateLease for Lease {}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(owned) = &self.0 {
            owned.store(false, Ordering::Release);
        }
    }
}

#[derive(Default)]
struct State {
    installer_owned: Arc<AtomicBool>,
    cache: RefCell<UpdateCacheState>,
    fail_release_highlights: bool,
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
        let mut cache = self.cache.borrow_mut();
        cache.observed_installed_version = Some(installed);
        cache.restart_needed = restart_needed;
        Ok(cache.clone())
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

struct Registry {
    scans: RefCell<VecDeque<Vec<InstanceInfo>>>,
    replacement_failures: RefCell<Vec<InstanceId>>,
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
struct Gateway {
    prepared: Vec<InstanceId>,
    released: Vec<InstanceId>,
    restarted: Vec<InstanceId>,
    block_at: Option<usize>,
    fail_prepare_at: Option<usize>,
    fail_restart: Option<InstanceId>,
}

impl UpdateParticipantGateway for Gateway {
    fn prepare(
        &mut self,
        participant: &InstanceInfo,
        _: &UpdatePrepareRequest,
    ) -> Result<UpdatePrepareReply, UpdateError> {
        let index = self.prepared.len();
        self.prepared.push(participant.instance_id);
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

impl HomebrewInstaller for Installer {
    fn upgrade(&mut self, _: &StableVersion) -> Result<StableVersion, UpdateError> {
        self.calls += 1;
        self.result.clone()
    }
}

#[test]
fn one_ten_and_fifteen_participants_install_and_restart_once() {
    for count in [1_usize, 10, 15] {
        let mut ids = TestIds::new(1_800_000_000_000);
        let identity = InstallationIdentity::from_digest([31; 32]);
        let participants = participants(&mut ids, identity, count);
        let initiating = participants[count / 2].instance_id;
        let registry = registry(participants.clone(), participants);
        let state = State::default();
        let mut gateway = Gateway::default();
        let mut installer = successful_installer();
        let operation_id = ids.request_id();
        let result = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
            .execute(
                operation_id,
                initiating,
                identity,
                &version("0.2.0"),
                Timestamp::from_millis(1_800_000_030_000),
                &(),
            )
            .expect("coordinate");
        assert_eq!(result.prepared_participants, count);
        assert_eq!(result.restart_requests, count);
        assert!(result.restart_failed.is_empty());
        assert_eq!(installer.calls, 1);
        assert_eq!(gateway.prepared.len(), count);
        assert_eq!(gateway.restarted.len(), count);
        assert_eq!(gateway.restarted.last(), Some(&initiating));
        assert!(!state.cache.borrow().restart_needed);
    }
}

#[test]
fn blocked_preflight_releases_ready_peers_before_installation() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([32; 32]);
    let participants = participants(&mut ids, identity, 4);
    let initiating = participants[0].instance_id;
    let registry = Registry {
        scans: RefCell::new(VecDeque::from([participants])),
        replacement_failures: RefCell::new(Vec::new()),
        fail_replacement_wait: false,
    };
    let state = State::default();
    let mut gateway = Gateway {
        block_at: Some(2),
        ..Gateway::default()
    };
    let mut installer = successful_installer();
    let result = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            initiating,
            identity,
            &version("0.2.0"),
            Timestamp::from_millis(1_800_000_030_000),
            &(),
        )
        .expect("abort");
    assert!(matches!(
        result.status,
        UpdateExecutionStatus::Aborted { ref code, .. } if code == "save_failed"
    ));
    assert_eq!(gateway.released.len(), 2);
    assert_eq!(installer.calls, 0);
}

#[test]
fn post_install_rescan_includes_new_sessions_and_records_partial_restart() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([33; 32]);
    let before = participants(&mut ids, identity, 2);
    let initiating = before[0].instance_id;
    let mut after = before.clone();
    after.extend(participants(&mut ids, identity, 1));
    let failed = after[1].instance_id;
    let registry = registry(before, after);
    let state = State::default();
    let mut gateway = Gateway {
        fail_restart: Some(failed),
        ..Gateway::default()
    };
    let mut installer = successful_installer();
    let result = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            initiating,
            identity,
            &version("0.2.0"),
            Timestamp::from_millis(1_800_000_030_000),
            &(),
        )
        .expect("partial restart");
    assert_eq!(result.prepared_participants, 2);
    assert_eq!(result.restart_requests, 3);
    assert_eq!(result.restart_failed, vec![failed]);
    assert!(state.cache.borrow().restart_needed);
}

#[test]
fn already_current_preflight_participants_are_released_after_installation() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([34; 32]);
    let mut current = participants(&mut ids, identity, 1);
    let initiating = current[0].instance_id;
    current[0].version = "0.2.0".to_owned();
    let registry = registry(current.clone(), current);
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();

    let result = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            initiating,
            identity,
            &version("0.2.0"),
            Timestamp::from_millis(1_800_000_030_000),
            &(),
        )
        .expect("coordinate current participant");

    assert_eq!(result.prepared_participants, 1);
    assert_eq!(result.restart_requests, 0);
    assert_eq!(gateway.released.len(), 1);
}

#[test]
fn unavailable_participant_aborts_and_releases_ready_peers() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([35; 32]);
    let participants = participants(&mut ids, identity, 3);
    let initiating = participants[0].instance_id;
    let registry = Registry {
        scans: RefCell::new(VecDeque::from([participants])),
        replacement_failures: RefCell::new(Vec::new()),
        fail_replacement_wait: false,
    };
    let state = State::default();
    let mut gateway = Gateway {
        fail_prepare_at: Some(1),
        ..Gateway::default()
    };
    let mut installer = successful_installer();

    let result = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            initiating,
            identity,
            &version("0.2.0"),
            Timestamp::from_millis(1_800_000_030_000),
            &(),
        )
        .expect("abort unavailable participant");

    assert!(matches!(
        result.status,
        UpdateExecutionStatus::Aborted { ref code, .. } if code == "participant_unavailable"
    ));
    assert_eq!(gateway.released.len(), 1);
    assert_eq!(installer.calls, 0);
}

fn registry(before: Vec<InstanceInfo>, after: Vec<InstanceInfo>) -> Registry {
    Registry {
        scans: RefCell::new(VecDeque::from([before, after])),
        replacement_failures: RefCell::new(Vec::new()),
        fail_replacement_wait: false,
    }
}

fn participants(
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
            }),
            launch_directory: "/workspace".to_owned(),
            started_at: Timestamp::from_millis(1_800_000_000_000),
        })
        .collect()
}

#[test]
fn protocol_ten_process_is_not_compatible_with_protocol_eleven_update_convergence() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([39; 32]);
    let mut participant = participants(&mut ids, identity, 1).remove(0);
    participant.storage_protocol = 10;
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

fn version(value: &str) -> StableVersion {
    StableVersion::parse(value).expect("stable version")
}
