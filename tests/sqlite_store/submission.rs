use super::*;

fn attempt(
    ids: &mut FakeIdGenerator,
    state: &AppState,
    thought_id: ThoughtId,
    digest: [u8; 32],
) -> SubmissionAttempt {
    SubmissionAttempt {
        id: ids.submission_id(),
        session_id: state.board.session.id,
        sources: vec![SubmissionSource {
            thought_id,
            source_digest: digest,
        }],
        payload_digest: digest,
        source_sequence: state.board.session.last_durable_sequence,
        disposition: SubmissionDisposition::RemoveAfterSuccess,
        route: proqi::ports::store::SubmissionJournalRoute::adjacent(Direction::Left),
        provider: "herdr".to_owned(),
        protocol: 19,
        target_fingerprint: [31; 32],
        pre_state: AgentState::Working,
        prepared_at: Timestamp::from_millis(20),
    }
}

fn state_of(connection: &Connection, id: proqi::domain::SubmissionId) -> String {
    connection
        .query_row(
            "SELECT state FROM submission_attempts WHERE id = ?1",
            [id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("submission state")
}

#[test]
fn journal_is_redacted_and_one_thought_has_only_one_active_attempt() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_000);
    let mut state = session_state(&mut ids, &test_path("submission-journal"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "ordinary thought", 2);
    let first = attempt(&mut ids, &state, thought_id, [7; 32]);
    let second = attempt(&mut ids, &state, thought_id, [8; 32]);

    store.prepare_submission(&first).expect("prepare first");
    assert!(matches!(
        store.prepare_submission(&second),
        Err(StoreError::Conflict(_))
    ));

    let connection = Connection::open(&fixture.config.database_path).expect("journal database");
    let columns: String = connection
        .query_row(
            "SELECT group_concat(name, ',') FROM pragma_table_info('submission_attempts')",
            [],
            |row| row.get(0),
        )
        .expect("journal columns");
    assert!(!columns.contains("content"));
    assert!(!columns.contains("pane_id"));
    assert!(!columns.contains("agent_session_id"));
    let stored_provider: String = connection
        .query_row(
            "SELECT provider FROM submission_attempts WHERE id = ?1",
            [first.id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("provider");
    assert_eq!(stored_provider, "herdr");
}

#[test]
fn current_adjacent_and_global_routes_round_trip_without_topology_identifiers() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_500);
    let mut state = session_state(&mut ids, &test_path("submission-route-journal"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let adjacent_id = create_thought(&mut store, &mut state, &mut ids, "adjacent", 2);
    let global_id = create_thought(&mut store, &mut state, &mut ids, "global", 3);
    let adjacent = attempt(&mut ids, &state, adjacent_id, [21; 32]);
    let mut global = attempt(&mut ids, &state, global_id, [22; 32]);
    global.route = proqi::ports::store::SubmissionJournalRoute::herdr_agent();
    store.prepare_submission(&adjacent).expect("adjacent route");
    store.prepare_submission(&global).expect("global route");

    let connection = Connection::open(&fixture.config.database_path).expect("journal database");
    let adjacent_route: (u32, String, Option<String>) = connection
        .query_row(
            "SELECT route_version, route_kind, direction FROM submission_attempts WHERE id = ?1",
            [adjacent.id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("adjacent route fields");
    let global_route: (u32, String, Option<String>) = connection
        .query_row(
            "SELECT route_version, route_kind, direction FROM submission_attempts WHERE id = ?1",
            [global.id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("global route fields");
    assert_eq!(
        adjacent_route,
        (1, "adjacent_pane".to_owned(), Some("left".to_owned()))
    );
    assert_eq!(global_route, (1, "herdr_agent".to_owned(), None));
    assert!(
        connection
            .execute(
                "UPDATE submission_attempts SET direction = 'right' WHERE id = ?1",
                [global.id.database_bytes().as_slice()],
            )
            .is_err()
    );
    assert!(
        connection
            .execute(
                "UPDATE submission_attempts SET route_version = 0 WHERE id = ?1",
                [global.id.database_bytes().as_slice()],
            )
            .is_err()
    );
}

#[test]
fn journal_transitions_are_compare_and_set_and_recovery_is_conservative() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(2_000);
    let mut state = session_state(&mut ids, &test_path("submission-recovery"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let prepared_thought = create_thought(&mut store, &mut state, &mut ids, "prepared", 2);
    let sending_thought = create_thought(&mut store, &mut state, &mut ids, "sending", 3);
    let prepared = attempt(&mut ids, &state, prepared_thought, [9; 32]);
    let sending = attempt(&mut ids, &state, sending_thought, [10; 32]);
    store.prepare_submission(&prepared).expect("prepare");
    store.prepare_submission(&sending).expect("prepare sending");
    store
        .mark_submission_sending(sending.id, Timestamp::from_millis(21))
        .expect("mark sending");
    assert!(matches!(
        store.mark_submission_sending(sending.id, Timestamp::from_millis(22)),
        Err(StoreError::Conflict(_))
    ));

    store
        .recover_submissions(state.board.session.id, Timestamp::from_millis(23))
        .expect("recover");
    let connection = Connection::open(&fixture.config.database_path).expect("journal database");
    assert_eq!(state_of(&connection, prepared.id), "cancelled");
    assert_eq!(state_of(&connection, sending.id), "outcome_unknown");
}

#[test]
fn restart_marks_a_multi_source_send_unknown_without_removing_sources() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(2_500);
    let mut state = session_state(&mut ids, &test_path("multi-submission-recovery"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let first = create_thought(&mut store, &mut state, &mut ids, "Grüße 👩‍💻", 2);
    let second = create_thought(&mut store, &mut state, &mut ids, "", 3);
    let mut sending = attempt(&mut ids, &state, first, [12; 32]);
    sending.sources = vec![
        SubmissionSource {
            thought_id: first,
            source_digest: [12; 32],
        },
        SubmissionSource {
            thought_id: second,
            source_digest: [13; 32],
        },
    ];
    store.prepare_submission(&sending).expect("prepare all");
    store
        .mark_submission_sending(sending.id, Timestamp::from_millis(21))
        .expect("mark sending");

    drop(store);
    let mut restarted = fixture.open();
    restarted
        .recover_submissions(session_id, Timestamp::from_millis(22))
        .expect("recover pending delivery");

    let snapshot = restarted
        .load_session(session_id)
        .expect("restart snapshot");
    assert_eq!(snapshot.board.live_thoughts().len(), 2);
    assert_eq!(snapshot.board.live_thoughts()[0].content, "Grüße 👩‍💻");
    assert_eq!(snapshot.board.live_thoughts()[1].content, "");
    let connection = Connection::open(&fixture.config.database_path).expect("journal database");
    assert_eq!(state_of(&connection, sending.id), "outcome_unknown");
}

#[test]
fn accepted_receipt_persists_advisory_state_and_deletion_identity() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(3_000);
    let mut state = session_state(&mut ids, &test_path("submission-accepted"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "accepted", 2);
    let record = attempt(&mut ids, &state, thought_id, [11; 32]);
    let deletion_id = ids.operation_id();
    store.prepare_submission(&record).expect("prepare");
    store
        .mark_submission_sending(record.id, Timestamp::from_millis(21))
        .expect("sending");
    assert!(matches!(
        store.finish_submission(
            record.id,
            &SubmissionOutcome {
                state: SubmissionAttemptState::Sending,
                post_state: None,
                error_code: None,
                deletion_operation_id: None,
                at: Timestamp::from_millis(22),
            },
        ),
        Err(StoreError::Integrity(_))
    ));
    store
        .finish_submission(
            record.id,
            &SubmissionOutcome {
                state: SubmissionAttemptState::Accepted,
                post_state: Some(AgentState::Unknown),
                error_code: None,
                deletion_operation_id: Some(deletion_id),
                at: Timestamp::from_millis(22),
            },
        )
        .expect("finish");

    let connection = Connection::open(&fixture.config.database_path).expect("journal database");
    let fields: (String, String, Vec<u8>) = connection
        .query_row(
            "SELECT state, post_state, deletion_operation_id
             FROM submission_attempts WHERE id = ?1",
            [record.id.database_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("accepted record");
    assert_eq!(fields.0, "accepted");
    assert_eq!(fields.1, "unknown");
    assert_eq!(fields.2, deletion_id.database_bytes());
}

#[test]
fn accepted_outcome_and_source_removal_commit_atomically() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(4_000);
    let mut state = session_state(&mut ids, &test_path("submission-atomic-removal"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "accepted", 2);
    let record = attempt(&mut ids, &state, thought_id, [14; 32]);
    store.prepare_submission(&record).expect("prepare");
    store
        .mark_submission_sending(record.id, Timestamp::from_millis(21))
        .expect("sending");
    reduce(
        &mut state,
        Action::BeginSubmission {
            thought_ids: vec![thought_id],
        },
    )
    .expect("lock source");
    let removal = one_effect(
        &mut state,
        Action::StageSubmissionRemoval {
            operation_id: ids.operation_id(),
            thought_ids: vec![thought_id],
            at: Timestamp::from_millis(22),
        },
    );
    let Effect::CommitBoardOperation(removal) = removal else {
        panic!("expected staged removal");
    };
    let outcome = SubmissionOutcome {
        state: SubmissionAttemptState::Accepted,
        post_state: Some(AgentState::Working),
        error_code: None,
        deletion_operation_id: Some(removal.id),
        at: Timestamp::from_millis(22),
    };

    let receipt = store
        .finish_submission_with_removal(record.id, &outcome, &removal)
        .expect("atomic finish");
    assert_eq!(receipt.sequence, removal.sequence);
    let replay = store
        .finish_submission_with_removal(record.id, &outcome, &removal)
        .expect("exact ambiguous retry");
    assert_eq!(replay.sequence, receipt.sequence);
    assert!(replay.idempotent_replay);
    let snapshot = store.load_session(session_id).expect("reload");
    assert!(snapshot.board.live_thoughts().is_empty());
    let connection = Connection::open(&fixture.config.database_path).expect("journal database");
    assert_eq!(state_of(&connection, record.id), "accepted");
}

#[test]
fn accepted_removal_replay_rejects_mismatched_outcome_or_operation() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(4_500);
    let mut state = session_state(&mut ids, &test_path("submission-removal-replay"));
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "accepted", 2);
    let record = attempt(&mut ids, &state, thought_id, [16; 32]);
    store.prepare_submission(&record).expect("prepare");
    store
        .mark_submission_sending(record.id, Timestamp::from_millis(21))
        .expect("sending");
    reduce(
        &mut state,
        Action::BeginSubmission {
            thought_ids: vec![thought_id],
        },
    )
    .expect("lock source");
    let removal = one_effect(
        &mut state,
        Action::StageSubmissionRemoval {
            operation_id: ids.operation_id(),
            thought_ids: vec![thought_id],
            at: Timestamp::from_millis(22),
        },
    );
    let Effect::CommitBoardOperation(removal) = removal else {
        panic!("expected staged removal");
    };
    let outcome = SubmissionOutcome {
        state: SubmissionAttemptState::Accepted,
        post_state: Some(AgentState::Working),
        error_code: None,
        deletion_operation_id: Some(removal.id),
        at: Timestamp::from_millis(22),
    };
    store
        .finish_submission_with_removal(record.id, &outcome, &removal)
        .expect("first finish");

    let mut mismatched_outcome = outcome.clone();
    mismatched_outcome.post_state = Some(AgentState::Unknown);
    assert!(matches!(
        store.finish_submission_with_removal(record.id, &mismatched_outcome, &removal),
        Err(StoreError::Conflict(_))
    ));
    let mut mismatched_removal = removal.clone();
    mismatched_removal.created_at = Timestamp::from_millis(23);
    assert!(matches!(
        store.finish_submission_with_removal(record.id, &outcome, &mismatched_removal),
        Err(StoreError::Conflict(_))
    ));
}

#[test]
fn failed_source_removal_rolls_back_the_accepted_outcome() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(5_000);
    let mut state = session_state(&mut ids, &test_path("submission-atomic-rollback"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "retained", 2);
    let record = attempt(&mut ids, &state, thought_id, [15; 32]);
    store.prepare_submission(&record).expect("prepare");
    store
        .mark_submission_sending(record.id, Timestamp::from_millis(21))
        .expect("sending");
    reduce(
        &mut state,
        Action::BeginSubmission {
            thought_ids: vec![thought_id],
        },
    )
    .expect("lock source");
    let removal = one_effect(
        &mut state,
        Action::StageSubmissionRemoval {
            operation_id: ids.operation_id(),
            thought_ids: vec![thought_id],
            at: Timestamp::from_millis(22),
        },
    );
    let Effect::CommitBoardOperation(mut removal) = removal else {
        panic!("expected staged removal");
    };
    removal.sequence = removal.sequence.checked_next().expect("sequence");
    let outcome = SubmissionOutcome {
        state: SubmissionAttemptState::Accepted,
        post_state: Some(AgentState::Working),
        error_code: None,
        deletion_operation_id: Some(removal.id),
        at: Timestamp::from_millis(22),
    };

    assert!(
        store
            .finish_submission_with_removal(record.id, &outcome, &removal)
            .is_err()
    );
    let snapshot = store.load_session(session_id).expect("reload");
    assert_eq!(snapshot.board.live_thoughts()[0].content, "retained");
    let connection = Connection::open(&fixture.config.database_path).expect("journal database");
    assert_eq!(state_of(&connection, record.id), "sending");
}
