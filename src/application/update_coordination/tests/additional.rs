use super::*;

#[test]
fn post_install_rescan_prepares_new_sessions_before_quiescence_and_restart() {
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
    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
            .execute(
                ids.request_id(),
                initiating,
                identity,
                &version("0.2.0"),
                Timestamp::from_millis(1_800_000_030_000),
                &(),
            )
            .expect("partial restart");
    assert_eq!(result.prepared_participants, 3);
    assert_eq!(result.quiesced_participants, 3);
    assert_eq!(result.restart_requests, 3);
    assert_eq!(result.restart_accepted, 2);
    assert_eq!(result.replacement_ready, 1);
    assert_eq!(result.restart_failed, vec![failed]);
    assert_eq!(gateway.released.len(), 2);
    assert!(state.cache.borrow().restart_needed);
    assert!(state.cache.borrow().release_highlights.is_none());
}

#[test]
fn post_install_preparation_rejection_marks_every_selected_session_for_manual_retry() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([46; 32]);
    let before = participants(&mut ids, identity, 2);
    let initiating = before[0].instance_id;
    let mut after = before.clone();
    after.extend(participants(&mut ids, identity, 2));
    let expected_failed = after
        .iter()
        .map(|participant| participant.instance_id)
        .collect::<Vec<_>>();
    let registry = registry(before, after);
    let state = State::default();
    let mut gateway = Gateway {
        block_at: Some(4),
        ..Gateway::default()
    };
    let mut installer = successful_installer();

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
            .execute(
                ids.request_id(),
                initiating,
                identity,
                &version("0.2.0"),
                Timestamp::from_millis(1_800_000_030_000),
                &(),
            )
            .expect("record bounded post-install rejection");

    assert_eq!(result.selected_participants, 4);
    assert_eq!(result.prepared_participants, 2);
    assert_eq!(result.restart_failed, expected_failed);
    assert_eq!(gateway.released.len(), 4);
    assert_eq!(result.resumable_sessions.len(), 4);
    assert!(state.cache.borrow().restart_needed);
    assert!(result.convergence_state_recorded);
}

struct Cancelled;

impl UpdateCancellation for Cancelled {
    fn is_cancelled(&self) -> bool {
        true
    }
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

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
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
    assert_eq!(gateway.released.len(), 2);
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
        fail_replacement_wait: false,
    };
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = Installer {
        calls: 0,
        result: Err(UpdateError::InstallerFailed),
    };

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
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
        fail_replacement_wait: false,
    };
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();
    let missing = ids.instance_id();

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
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
    let registry = registry(before.clone(), before.clone());
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();

    UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
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
fn failed_coordinator_quiescence_retains_exact_manual_resume_state() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([38; 32]);
    let before = participants(&mut ids, identity, 1);
    let initiating = before[0].instance_id;
    let registry = registry(before.clone(), before.clone());
    let state = State::default();
    let mut gateway = Gateway {
        fail_quiesce: Some(initiating),
        ..Gateway::default()
    };
    let mut installer = successful_installer();

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
            .execute(
                ids.request_id(),
                initiating,
                identity,
                &version("0.2.0"),
                Timestamp::from_millis(1_800_000_030_000),
                &(),
            )
            .expect("record missing coordinator");

    assert_eq!(result.restart_requests, 0);
    assert_eq!(result.restart_failed, vec![initiating]);
    assert_eq!(result.quiescence_failed, vec![initiating]);
    assert_eq!(result.resumable_sessions, vec![before[0].session_id]);
    assert!(state.cache.borrow().restart_needed);
    assert!(state.cache.borrow().release_highlights.is_none());
}

#[test]
fn incomplete_peer_replacement_creates_no_false_highlight_announcement() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([39; 32]);
    let before = participants(&mut ids, identity, 2);
    let initiating = before[0].instance_id;
    let peer = before[1].instance_id;
    let registry = Registry {
        scans: RefCell::new(VecDeque::from([before.clone(), before.clone()])),
        replacement_failures: RefCell::new(vec![peer]),
        fail_replacement_wait: false,
    };
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
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
    assert!(!result.restart_failed.contains(&initiating));
    assert_eq!(result.resumable_sessions, vec![before[1].session_id]);
    assert_eq!(gateway.released.len(), 2);
    assert_eq!(gateway.restarted, vec![peer, initiating]);
    assert!(state.cache.borrow().release_highlights.is_none());
}

#[test]
fn replacement_scan_failure_is_bounded_after_quiescence() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([44; 32]);
    let before = participants(&mut ids, identity, 2);
    let initiating = before[0].instance_id;
    let peer = before[1].instance_id;
    let mut registry = registry(before.clone(), before.clone());
    registry.fail_replacement_wait = true;
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
            .execute(
                ids.request_id(),
                initiating,
                identity,
                &version("0.2.0"),
                Timestamp::from_millis(1_800_000_030_000),
                &(),
            );

    let result = result.expect("bounded incomplete result");
    assert_eq!(result.restart_failed, vec![peer]);
    assert_eq!(result.resumable_sessions, vec![before[1].session_id]);
    assert_eq!(gateway.restarted, vec![peer, initiating]);
    assert_eq!(gateway.released.len(), 2);
    assert!(state.cache.borrow().release_highlights.is_none());
    assert!(state.cache.borrow().restart_needed);
}

#[test]
fn successful_peer_convergence_targets_only_the_initiating_session() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([40; 32]);
    let before = participants(&mut ids, identity, 2);
    let initiating = before[0].instance_id;
    let initiating_session = before[0].session_id;
    let peer_session = before[1].session_id;
    let registry = registry(before.clone(), before.clone());
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();

    UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
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
fn failed_pending_write_restarts_the_quiesced_initiator_without_a_false_announcement() {
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

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
            .execute(
                ids.request_id(),
                initiating,
                identity,
                &version("0.2.0"),
                Timestamp::from_millis(1_800_000_030_000),
                &(),
            )
            .expect("failed pending write stays bounded");

    assert_eq!(result.restart_requests, 1);
    assert!(result.restart_failed.is_empty());
    assert!(!result.convergence_state_recorded);
    assert_eq!(gateway.released.len(), 1);
    assert_eq!(gateway.restarted, vec![initiating]);
    assert!(state.cache.borrow().release_highlights.is_none());
    assert!(state.cache.borrow().restart_needed);
}

#[test]
fn rejected_initiating_restart_discards_unreachable_highlight_announcement() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([45; 32]);
    let before = participants(&mut ids, identity, 1);
    let initiating = before[0].instance_id;
    let registry = registry(before.clone(), before.clone());
    let state = State::default();
    let mut gateway = Gateway {
        fail_restart: Some(initiating),
        ..Gateway::default()
    };
    let mut installer = successful_installer();

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
            .execute(
                ids.request_id(),
                initiating,
                identity,
                &version("0.2.0"),
                Timestamp::from_millis(1_800_000_030_000),
                &(),
            )
            .expect("rejected initiating restart");

    assert_eq!(result.restart_requests, 1);
    assert_eq!(result.restart_accepted, 0);
    assert_eq!(result.restart_failed, vec![initiating]);
    assert_eq!(result.resumable_sessions, vec![before[0].session_id]);
    assert_eq!(gateway.released.len(), 1);
    assert!(state.cache.borrow().restart_needed);
    assert!(state.cache.borrow().release_highlights.is_none());
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
        fail_replacement_wait: false,
    };
    let state = State::default();
    let mut gateway = Gateway::default();
    let mut installer = Installer {
        calls: 0,
        result: Ok(version("0.3.0")),
    };

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
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
