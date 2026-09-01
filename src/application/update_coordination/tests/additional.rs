use super::*;

struct Cancelled;

impl UpdateCancellation for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
}

#[test]
fn installer_failure_releases_every_ready_participant() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([36; 32]);
    let participants = participants(&mut ids, identity, 3);
    let initiating = participants[0].instance_id;
    let registry = Registry {
        scans: RefCell::new(VecDeque::from([participants])),
        replacement_failures: RefCell::new(Vec::new()),
    };
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = Installer {
        calls: 0,
        result: Err(UpdateError::InstallerFailed),
    };

    let result = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            initiating,
            identity,
            &version("0.2.0"),
            Timestamp::from_millis(1_800_000_030_000),
            &(),
        );

    assert_eq!(result, Err(UpdateError::InstallerFailed));
    assert_eq!(gateway.released.len(), 3);
    assert!(gateway.restarted.is_empty());
    assert!(state.cache.borrow().release_highlights.is_none());
}

#[test]
fn unregistered_coordinator_aborts_before_installation() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([37; 32]);
    let participants = participants(&mut ids, identity, 2);
    let registry = Registry {
        scans: RefCell::new(VecDeque::from([participants])),
        replacement_failures: RefCell::new(Vec::new()),
    };
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();
    let missing = ids.instance_id();

    let result = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            missing,
            identity,
            &version("0.2.0"),
            Timestamp::from_millis(1_800_000_030_000),
            &(),
        )
        .expect("abort unregistered coordinator");

    assert!(matches!(
        result.status,
        UpdateExecutionStatus::Aborted { blocker, ref code }
            if blocker == Some(missing) && code == "coordinator_not_registered"
    ));
    assert_eq!(installer.calls, 0);
}

#[test]
fn cancelled_single_session_restart_creates_no_automatic_highlights() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([42; 32]);
    let before = participants(&mut ids, identity, 1);
    let initiating = before[0].instance_id;
    let registry = registry(before.clone(), before);
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();

    UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            initiating,
            identity,
            &version("0.2.0"),
            Timestamp::from_millis(1_800_000_030_000),
            &Cancelled,
        )
        .expect("cancelled restart remains bounded");

    assert!(state.cache.borrow().release_highlights.is_none());
}

#[test]
fn coordinator_missing_after_installation_retains_pending_for_delayed_resume() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([38; 32]);
    let before = participants(&mut ids, identity, 1);
    let initiating = before[0].instance_id;
    let initiating_session = before[0].session_id;
    let registry = registry(before, Vec::new());
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
        .expect("record missing coordinator");

    assert_eq!(result.restart_requests, 1);
    assert_eq!(result.restart_failed, vec![initiating]);
    assert!(state.cache.borrow().restart_needed);
    assert_eq!(
        state
            .cache
            .borrow()
            .release_highlights
            .as_ref()
            .map(ReleaseHighlightAnnouncement::session_id),
        Some(initiating_session)
    );
}

#[test]
fn incomplete_peer_replacement_suppresses_automatic_highlights() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([39; 32]);
    let before = participants(&mut ids, identity, 2);
    let initiating = before[0].instance_id;
    let peer = before[1].instance_id;
    let registry = Registry {
        scans: RefCell::new(VecDeque::from([before.clone(), before])),
        replacement_failures: RefCell::new(vec![peer]),
    };
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
        .expect("partial replacement");

    assert!(result.restart_failed.contains(&peer));
    assert!(state.cache.borrow().release_highlights.is_none());
}

#[test]
fn successful_peer_convergence_targets_only_the_initiating_session() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([40; 32]);
    let before = participants(&mut ids, identity, 2);
    let initiating = before[0].instance_id;
    let initiating_session = before[0].session_id;
    let peer_session = before[1].session_id;
    let registry = registry(before.clone(), before);
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();

    UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            initiating,
            identity,
            &version("0.2.0"),
            Timestamp::from_millis(1_800_000_030_000),
            &(),
        )
        .expect("converged update");

    let cache = state.cache.borrow();
    let announcement = cache
        .release_highlights
        .as_ref()
        .expect("pending highlights");
    assert_eq!(announcement.session_id(), initiating_session);
    assert_ne!(announcement.session_id(), peer_session);
    assert_eq!(announcement.previous_version(), &version("0.1.0"));
    assert_eq!(announcement.target_version(), &version("0.2.0"));
    assert!(!announcement.acknowledged());
}

#[test]
fn failed_pending_write_keeps_the_initiator_running_without_a_false_announcement() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([43; 32]);
    let before = participants(&mut ids, identity, 1);
    let initiating = before[0].instance_id;
    let registry = registry(before.clone(), before);
    let state = State {
        fail_release_highlights: true,
        ..State::default()
    };
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
        .expect("failed pending write stays bounded");

    assert_eq!(result.restart_requests, 0);
    assert_eq!(result.restart_failed, vec![initiating]);
    assert!(!result.convergence_state_recorded);
    assert_eq!(gateway.released, vec![initiating]);
    assert!(gateway.restarted.is_empty());
    assert!(state.cache.borrow().release_highlights.is_none());
    assert!(state.cache.borrow().restart_needed);
}

#[test]
fn mismatched_installer_result_creates_no_announcement_or_restart() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([41; 32]);
    let before = participants(&mut ids, identity, 2);
    let initiating = before[0].instance_id;
    let registry = Registry {
        scans: RefCell::new(VecDeque::from([before])),
        replacement_failures: RefCell::new(Vec::new()),
    };
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = Installer {
        calls: 0,
        result: Ok(version("0.3.0")),
    };

    let result = UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer)
        .execute(
            ids.request_id(),
            initiating,
            identity,
            &version("0.2.0"),
            Timestamp::from_millis(1_800_000_030_000),
            &(),
        );

    assert_eq!(result, Err(UpdateError::InstallerFailed));
    assert!(gateway.restarted.is_empty());
    assert!(state.cache.borrow().release_highlights.is_none());
}
