use super::*;

#[test]
fn pending_external_restart_blocks_installer_before_participant_preparation() {
    let mut ids = TestIds::new(1_800_000_000_000);
    let identity = InstallationIdentity::from_digest([30; 32]);
    let participants = participants(&mut ids, identity, 2);
    let initiating = participants[0].instance_id;
    let registry = registry(participants.clone(), participants);
    let pending_target = version("0.2.0");
    let pending = ExternalRestartPending::new(
        pending_target.clone(),
        ids.request_id(),
        vec![ExternalRestartExpectation::new(
            ids.session_id(),
            ids.instance_id(),
            42,
            version("0.1.0"),
        )],
    )
    .expect("pending external restart");
    let state = State::default();
    state.cache.replace(UpdateCacheState {
        observed_installed_version: Some(pending_target),
        restart_needed: true,
        external_restart: Some(pending),
        ..UpdateCacheState::default()
    });
    let mut gateway = Gateway::default();
    let mut installer = successful_installer();

    let result =
        UpdateRestartCoordinator::new(&state, &registry, &mut gateway, &mut installer, &clock())
            .execute(
                ids.request_id(),
                initiating,
                identity,
                &version("0.3.0"),
                Timestamp::from_millis(1_800_000_030_000),
                &(),
            )
            .expect("pending external restart abort");

    assert!(matches!(
        result.status,
        UpdateExecutionStatus::Aborted { ref code, blocker: None }
            if code == "external_restart_pending"
    ));
    assert_eq!(installer.calls, 0);
    assert!(gateway.prepared.is_empty());
    assert!(gateway.released.is_empty());
    assert!(state.restart_writes.borrow().is_empty());
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
        let result = UpdateRestartCoordinator::new(
            &state,
            &registry,
            &mut gateway,
            &mut installer,
            &clock(),
        )
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
        assert_eq!(result.selected_participants, count);
        assert_eq!(result.restart_requests, count);
        assert_eq!(result.restart_accepted, count);
        assert_eq!(result.replacement_ready, count.saturating_sub(1));
        assert_eq!(result.replacement_missing, 0);
        assert!(result.restart_failed.is_empty());
        assert_eq!(installer.calls, 1);
        assert_eq!(gateway.prepared.len(), count.saturating_mul(2));
        assert_eq!(gateway.released.len(), count);
        assert_eq!(gateway.restarted.len(), count);
        assert_eq!(gateway.restarted.last(), Some(&initiating));
        assert!(state.cache.borrow().restart_needed);
        assert_eq!(&*state.restart_writes.borrow(), &[true]);
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
            .expect("abort");
    assert!(matches!(
        result.status,
        UpdateExecutionStatus::Aborted { ref code, .. } if code == "save_failed"
    ));
    assert_eq!(gateway.released.len(), 2);
    assert_eq!(installer.calls, 0);
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
            .expect("abort unavailable participant");

    assert!(matches!(
        result.status,
        UpdateExecutionStatus::Aborted { ref code, .. } if code == "participant_unavailable"
    ));
    assert_eq!(gateway.released.len(), 1);
    assert_eq!(installer.calls, 0);
}
