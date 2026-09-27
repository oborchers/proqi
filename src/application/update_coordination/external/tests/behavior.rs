use crate::{
    application::test_support::TestIds,
    application::update_coordination::{
        ExternalUpgradeFailure,
        tests::{Gateway, Lease, State, participants, registry, version},
    },
    domain::{InstallationIdentity, InstallationKind, Timestamp},
    ports::{
        environment::IdGenerator as _,
        runtime::Lease as RuntimeLease,
        update::{
            ExternalUpgradeAdoptionAuthority, ExternalUpgradeAuthorityError, UpdateCancellation,
            UpdateError, UpdateStateStore as _,
        },
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use super::super::{ExternalUpgradeCacheStatus, ExternalUpgradeCoordinator};

#[derive(Default)]
struct AdoptionAuthority;

struct AdoptionLease;

impl RuntimeLease for AdoptionLease {}

impl ExternalUpgradeAdoptionAuthority for AdoptionAuthority {
    fn revalidate_installation(&mut self) -> Result<(), ExternalUpgradeAuthorityError> {
        Ok(())
    }

    fn acquire(&mut self) -> Result<Box<dyn RuntimeLease>, ExternalUpgradeAuthorityError> {
        Ok(Box::new(AdoptionLease))
    }
}

#[test]
fn compatible_older_sessions_reuse_prepare_quiesce_restart_and_exact_wait() {
    let mut ids = TestIds::new(1_800_400_000_000);
    let installation = InstallationIdentity::from_digest([51; 32]);
    let old = version("0.10.0");
    let current = version("0.11.0");
    let owners = participants(&mut ids, installation, 3);
    let registry = registry(owners.clone(), Vec::new());
    let state = State::default();
    state
        .record_restart_state(installation, old.clone(), true)
        .expect("stale external observation");
    let mut gateway = Gateway::default();

    let admission = ExternalUpgradeCoordinator::new(&state, &registry, &mut gateway)
        .reconcile(
            ids.request_id(),
            installation,
            InstallationKind::HomebrewFormula,
            &old,
            &current,
            Timestamp::from_millis(1_800_400_030_000),
            Box::new(Lease(None)),
            &(),
            &mut AdoptionAuthority,
        )
        .expect("compatible external convergence");

    assert_eq!(gateway.prepared.len(), owners.len());
    assert_eq!(gateway.quiesced.len(), owners.len());
    assert_eq!(gateway.restarted.len(), owners.len());
    let cache = state.load(installation).expect("converged cache");
    assert_eq!(cache.observed_installed_version, Some(current));
    assert!(!cache.restart_needed);
    drop(admission);
}

#[test]
fn preparation_or_quiescence_failure_never_adopts_the_new_observation() {
    for failure in [Failure::Prepare, Failure::Quiesce] {
        let mut ids = TestIds::new(1_800_500_000_000);
        let installation = InstallationIdentity::from_digest([52; 32]);
        let old = version("0.10.0");
        let current = version("0.11.0");
        let owners = participants(&mut ids, installation, 2);
        let registry = registry(owners.clone(), Vec::new());
        let state = State::default();
        state
            .record_restart_state(installation, old.clone(), true)
            .expect("stale external observation");
        let mut gateway = Gateway::default();
        match failure {
            Failure::Prepare => gateway.fail_prepare_at = Some(1),
            Failure::Quiesce => {
                gateway.fail_quiesce = Some(owners[1].instance_id);
                gateway.fail_restart = Some(owners[0].instance_id);
            }
        }

        let result = ExternalUpgradeCoordinator::new(&state, &registry, &mut gateway).reconcile(
            ids.request_id(),
            installation,
            InstallationKind::HomebrewFormula,
            &old,
            &current,
            Timestamp::from_millis(1_800_500_030_000),
            Box::new(Lease(None)),
            &(),
            &mut AdoptionAuthority,
        );

        let blockers = match failure {
            Failure::Prepare => {
                let Err(ExternalUpgradeFailure::Blocked(blockers)) = result else {
                    panic!("preparation failure must block external convergence");
                };
                assert_eq!(blockers.len(), 1);
                assert_eq!(blockers[0].instance_id, owners[1].instance_id);
                assert!(gateway.restarted.is_empty());
                blockers
            }
            Failure::Quiesce => {
                let Err(ExternalUpgradeFailure::Incomplete(blockers)) = result else {
                    panic!("partial quiescence must report every affected session");
                };
                assert_eq!(gateway.restarted, [owners[0].instance_id]);
                blockers
            }
        };
        if matches!(failure, Failure::Prepare) {
            assert_eq!(blockers.len(), 1);
            assert_eq!(blockers[0].instance_id, owners[1].instance_id);
        } else {
            assert_eq!(blockers.len(), 2);
            assert!(
                blockers
                    .iter()
                    .any(|blocker| blocker.instance_id == owners[0].instance_id
                        && blocker.session_id == owners[0].session_id)
            );
            assert!(
                blockers
                    .iter()
                    .any(|blocker| blocker.instance_id == owners[1].instance_id
                        && blocker.session_id == owners[1].session_id)
            );
        }
        let cache = state.load(installation).expect("unchanged cache");
        assert_eq!(cache.observed_installed_version, Some(old));
        assert!(cache.restart_needed);
    }
}

#[derive(Clone, Copy)]
enum Failure {
    Prepare,
    Quiesce,
}

#[test]
fn no_owner_adoption_is_deferred_and_cancellation_fails_before_preparation() {
    let mut ids = TestIds::new(1_800_600_000_000);
    let installation = InstallationIdentity::from_digest([53; 32]);
    let old = version("0.9.0");
    let current = version("0.10.0");
    let state = State::default();
    state
        .record_restart_state(installation, old.clone(), true)
        .expect("stale observation");
    let empty = registry(Vec::new(), Vec::new());
    let mut gateway = Gateway::default();
    let deferred = ExternalUpgradeCoordinator::new(&state, &empty, &mut gateway)
        .reconcile(
            ids.request_id(),
            installation,
            InstallationKind::HomebrewFormula,
            &old,
            &current,
            Timestamp::from_millis(1_800_600_030_000),
            Box::new(Lease(None)),
            &(),
            &mut AdoptionAuthority,
        )
        .expect("uncontended adoption is deferred until real store readiness");
    let (lease, status, adoption) = deferred.into_parts();
    assert_eq!(status, ExternalUpgradeCacheStatus::DeferredUntilStoreReady);
    assert!(adoption.is_some());
    drop(lease);
    assert_eq!(
        state
            .load(installation)
            .expect("unchanged state")
            .observed_installed_version,
        Some(old.clone())
    );
    let owners = participants(&mut ids, installation, 1);
    let registry = registry(owners, Vec::new());
    let state = State::default();
    state
        .record_restart_state(installation, old.clone(), true)
        .expect("stale observation");
    let mut gateway = Gateway::default();
    let cancellation = ExternalUpgradeCoordinator::new(&state, &registry, &mut gateway).reconcile(
        ids.request_id(),
        installation,
        InstallationKind::HomebrewFormula,
        &old,
        &current,
        Timestamp::from_millis(1_800_600_030_000),
        Box::new(Lease(None)),
        &Cancelled,
        &mut AdoptionAuthority,
    );
    assert!(matches!(
        cancellation,
        Err(ExternalUpgradeFailure::Update(_))
    ));
    assert!(gateway.prepared.is_empty());
    assert_eq!(
        state
            .load(installation)
            .expect("cancelled state")
            .observed_installed_version,
        Some(old)
    );
}

struct Cancelled;

impl UpdateCancellation for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

#[test]
fn missing_exact_replacement_keeps_restart_state_for_truthful_retry() {
    let mut ids = TestIds::new(1_800_700_000_000);
    let installation = InstallationIdentity::from_digest([54; 32]);
    let old = version("0.10.0");
    let current = version("0.11.0");
    let owners = participants(&mut ids, installation, 1);
    let registry = registry(owners.clone(), Vec::new());
    *registry.replacement_failures.borrow_mut() = vec![owners[0].instance_id];
    let state = State::default();
    state
        .record_restart_state(installation, old.clone(), true)
        .expect("stale external observation");
    let mut gateway = Gateway::default();

    let result = ExternalUpgradeCoordinator::new(&state, &registry, &mut gateway).reconcile(
        ids.request_id(),
        installation,
        InstallationKind::HomebrewFormula,
        &old,
        &current,
        Timestamp::from_millis(1_800_700_030_000),
        Box::new(Lease(None)),
        &(),
        &mut AdoptionAuthority,
    );

    let Err(ExternalUpgradeFailure::Incomplete(blockers)) = result else {
        panic!("missing replacement must stay incomplete");
    };
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].session_id, owners[0].session_id);
    let cache = state.load(installation).expect("pending restart cache");
    assert_eq!(cache.observed_installed_version, Some(current));
    assert!(cache.restart_needed);
}

#[test]
fn failed_restart_completion_is_revalidated_and_idempotently_repaired() {
    let mut ids = TestIds::new(1_800_800_000_000);
    let installation = InstallationIdentity::from_digest([55; 32]);
    let old = version("0.10.0");
    let current = version("0.11.0");
    let owners = participants(&mut ids, installation, 2);
    let mut replacements = owners.clone();
    for replacement in &mut replacements {
        replacement.version = current.to_string();
    }
    let registry = registry(owners, replacements);
    let mut state = State::default();
    state.fail_external_completion = true;
    state
        .record_restart_state(installation, old.clone(), true)
        .expect("stale observation");
    let mut gateway = Gateway::default();

    let first = ExternalUpgradeCoordinator::new(&state, &registry, &mut gateway).reconcile(
        ids.request_id(),
        installation,
        InstallationKind::HomebrewFormula,
        &old,
        &current,
        Timestamp::from_millis(1_800_800_030_000),
        Box::new(Lease(None)),
        &(),
        &mut AdoptionAuthority,
    );
    assert!(matches!(first, Err(ExternalUpgradeFailure::Update(_))));
    let pending = state.load(installation).expect("pending cache");
    assert_eq!(pending.observed_installed_version, Some(current.clone()));
    assert!(pending.restart_needed);
    let exact_pending = pending
        .external_restart
        .as_ref()
        .expect("exact external restart authority");

    state.fail_external_completion = false;
    let admission = ExternalUpgradeCoordinator::new(&state, &registry, &mut gateway)
        .complete_pending_restart(
            installation,
            InstallationKind::HomebrewFormula,
            &current,
            exact_pending,
            Box::new(Lease(None)),
            &mut AdoptionAuthority,
        )
        .expect("revalidated completion retry");
    drop(admission);
    assert!(
        !state
            .load(installation)
            .expect("repaired cache")
            .restart_needed
    );
}

#[test]
fn persistence_failure_after_quiescence_attempts_exact_recovery_without_admission() {
    let mut ids = TestIds::new(1_800_900_000_000);
    let installation = InstallationIdentity::from_digest([56; 32]);
    let old = version("0.10.0");
    let current = version("0.11.0");
    let owners = participants(&mut ids, installation, 2);
    let registry = registry(owners.clone(), Vec::new());
    let mut state = State::default();
    state.fail_external_reconcile = true;
    state
        .record_restart_state(installation, old.clone(), true)
        .expect("stale observation");
    let mut gateway = Gateway::default();

    let result = ExternalUpgradeCoordinator::new(&state, &registry, &mut gateway).reconcile(
        ids.request_id(),
        installation,
        InstallationKind::HomebrewFormula,
        &old,
        &current,
        Timestamp::from_millis(1_800_900_030_000),
        Box::new(Lease(None)),
        &(),
        &mut AdoptionAuthority,
    );

    let Err(ExternalUpgradeFailure::Persistence { blockers, .. }) = result else {
        panic!("post-quiescence persistence failure must retain exact recovery identities");
    };
    assert_eq!(gateway.restarted.len(), owners.len());
    assert_eq!(blockers.len(), owners.len());
    for owner in &owners {
        assert!(
            blockers.iter().any(|blocker| {
                blocker.instance_id == owner.instance_id && blocker.session_id == owner.session_id
            }),
            "every quiesced owner remains explicitly recoverable"
        );
    }
    let cache = state.load(installation).expect("unchanged cache");
    assert_eq!(cache.observed_installed_version, Some(old));
    assert!(cache.restart_needed);
}

#[test]
fn active_installation_switch_during_quiescence_recovers_without_cache_adoption() {
    let mut ids = TestIds::new(1_800_950_000_000);
    let installation = InstallationIdentity::from_digest([58; 32]);
    let old = version("0.10.0");
    let current = version("0.11.0");
    let owners = participants(&mut ids, installation, 2);
    let registry = registry(owners.clone(), Vec::new());
    let state = State::default();
    state
        .record_restart_state(installation, old.clone(), true)
        .expect("stale observation");
    let changed = Arc::new(AtomicBool::new(false));
    let mut gateway = Gateway::default();
    gateway.cancel_after_quiesce = Some(Arc::clone(&changed));
    let mut authority = ChangedInstallationAuthority(Arc::clone(&changed));

    let result = ExternalUpgradeCoordinator::new(&state, &registry, &mut gateway).reconcile(
        ids.request_id(),
        installation,
        InstallationKind::HomebrewFormula,
        &old,
        &current,
        Timestamp::from_millis(1_800_950_030_000),
        Box::new(Lease(None)),
        &(),
        &mut authority,
    );

    let Err(ExternalUpgradeFailure::Authority { blockers, source }) = result else {
        panic!("an active-link switch must fail before cache adoption");
    };
    assert!(matches!(
        source,
        ExternalUpgradeAuthorityError::Update(UpdateError::Installation(_))
    ));
    assert_eq!(blockers.len(), owners.len());
    assert_eq!(gateway.restarted.len(), owners.len());
    let cache = state.load(installation).expect("unchanged cache");
    assert_eq!(cache.observed_installed_version, Some(old));
    assert!(cache.restart_needed);
    assert!(cache.external_restart.is_none());
}

struct ChangedInstallationAuthority(Arc<AtomicBool>);

impl ExternalUpgradeAdoptionAuthority for ChangedInstallationAuthority {
    fn establish(&mut self) -> Result<(), ExternalUpgradeAuthorityError> {
        assert!(
            !self.0.load(Ordering::Acquire),
            "installation identity is established before quiescence"
        );
        Ok(())
    }

    fn revalidate_installation(&mut self) -> Result<(), ExternalUpgradeAuthorityError> {
        Err(
            UpdateError::Installation("active Homebrew link changed during convergence".to_owned())
                .into(),
        )
    }

    fn acquire(&mut self) -> Result<Box<dyn RuntimeLease>, ExternalUpgradeAuthorityError> {
        assert!(
            self.0.load(Ordering::Acquire),
            "installation must be revalidated only after quiescence"
        );
        self.revalidate_installation()?;
        Ok(Box::new(AdoptionLease))
    }
}

#[test]
fn cancellation_after_quiescence_does_not_interrupt_exact_replacement_cleanup() {
    let mut ids = TestIds::new(1_801_000_000_000);
    let installation = InstallationIdentity::from_digest([57; 32]);
    let old = version("0.10.0");
    let current = version("0.11.0");
    let owners = participants(&mut ids, installation, 1);
    let registry = registry(owners.clone(), Vec::new());
    let state = State::default();
    state
        .record_restart_state(installation, old.clone(), true)
        .expect("stale observation");
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut gateway = Gateway::default();
    gateway.cancel_after_quiesce = Some(Arc::clone(&cancelled));
    let cancellation = FlagCancellation(cancelled);

    let admission = ExternalUpgradeCoordinator::new(&state, &registry, &mut gateway)
        .reconcile(
            ids.request_id(),
            installation,
            InstallationKind::HomebrewFormula,
            &old,
            &current,
            Timestamp::from_millis(1_801_000_030_000),
            Box::new(Lease(None)),
            &cancellation,
            &mut AdoptionAuthority,
        )
        .expect("irreversible cleanup completes despite later cancellation");
    drop(admission);
    assert_eq!(gateway.restarted, [owners[0].instance_id]);
    assert!(!state.load(installation).expect("cache").restart_needed);
}

struct FlagCancellation(Arc<AtomicBool>);

impl UpdateCancellation for FlagCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
