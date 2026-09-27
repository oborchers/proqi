use crate::{
    application::{
        test_support::TestIds,
        update_coordination::{
            ExternalUpgradeBlockerReason, ExternalUpgradeFailure,
            tests::{Gateway, Lease, State, participants, registry, version},
        },
    },
    domain::{
        EXTERNAL_RESTART_MAX_EXPECTATIONS, InstallationIdentity, InstallationKind, Timestamp,
    },
    ports::{
        environment::IdGenerator as _,
        runtime::Lease as RuntimeLease,
        update::{
            ExternalUpgradeAdoptionAuthority, ExternalUpgradeAuthorityError, UpdateCancellation,
            UpdateStateStore as _,
        },
    },
};

use super::super::ExternalUpgradeCoordinator;

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

struct Cancelled;

impl UpdateCancellation for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

#[test]
fn compatible_cohort_capacity_accepts_32_and_blocks_33_before_preparation() {
    let mut ids = TestIds::new(1_800_650_000_000);
    let installation = InstallationIdentity::from_digest([59; 32]);
    let old = version("0.10.0");
    let current = version("0.11.0");
    let boundary = participants(&mut ids, installation, EXTERNAL_RESTART_MAX_EXPECTATIONS);
    let boundary_registry = registry(boundary, Vec::new());
    let boundary_state = State::default();
    boundary_state
        .record_restart_state(installation, old.clone(), true)
        .expect("boundary external observation");
    let mut boundary_gateway = Gateway::default();
    let boundary_result =
        ExternalUpgradeCoordinator::new(&boundary_state, &boundary_registry, &mut boundary_gateway)
            .reconcile(
                ids.request_id(),
                installation,
                InstallationKind::HomebrewFormula,
                &old,
                &current,
                Timestamp::from_millis(1_800_650_020_000),
                Box::new(Lease(None)),
                &Cancelled,
                &mut AdoptionAuthority,
            );
    assert!(matches!(
        boundary_result,
        Err(ExternalUpgradeFailure::Update(_))
    ));
    assert!(boundary_gateway.prepared.is_empty());

    let owner_count = EXTERNAL_RESTART_MAX_EXPECTATIONS + 1;
    let owners = participants(&mut ids, installation, owner_count);
    let registry = registry(owners, Vec::new());
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
        Timestamp::from_millis(1_800_650_030_000),
        Box::new(Lease(None)),
        &(),
        &mut AdoptionAuthority,
    );

    let Err(ExternalUpgradeFailure::Capacity {
        blockers,
        participant_count,
        maximum,
    }) = result
    else {
        panic!("oversized cohort must fail with typed capacity evidence");
    };
    assert_eq!(participant_count, owner_count);
    assert_eq!(maximum, EXTERNAL_RESTART_MAX_EXPECTATIONS);
    assert_eq!(blockers.len(), owner_count);
    assert!(
        blockers.iter().all(|blocker| {
            blocker.reason == ExternalUpgradeBlockerReason::CohortCapacityExceeded
        })
    );
    assert!(gateway.prepared.is_empty());
    assert!(gateway.quiesced.is_empty());
    assert!(gateway.restarted.is_empty());
    let cache = state.load(installation).expect("unchanged cache");
    assert_eq!(cache.observed_installed_version, Some(old));
    assert!(cache.restart_needed);
}
