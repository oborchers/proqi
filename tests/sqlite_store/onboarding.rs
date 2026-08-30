use super::*;

fn candidate(
    ids: &mut FakeIdGenerator,
    path: &Path,
    environment: FirstRunEnvironment,
) -> FirstRunBoard {
    let session = Session::new(
        ids.session_id(),
        path.to_path_buf(),
        Timestamp::from_millis(1),
    )
    .expect("session");
    first_run_board(session, ids, environment).expect("practice board")
}

fn contents(store: &mut SqliteStore, board: &FirstRunBoard) -> Vec<String> {
    store
        .load_session(board.board().session.id)
        .expect("session snapshot")
        .board
        .live_thoughts()
        .into_iter()
        .map(|thought| thought.content.clone())
        .collect()
}

#[test]
fn first_claim_seeds_six_ordinary_searchable_thoughts_and_later_sessions_are_empty() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let first = candidate(
        &mut ids,
        &test_path("proqi-onboarding-first"),
        FirstRunEnvironment::Standalone,
    );
    let second = candidate(
        &mut ids,
        &test_path("proqi-onboarding-second"),
        FirstRunEnvironment::HerdrManaged,
    );

    assert_eq!(
        store.create_first_run_session(&first).expect("first claim"),
        FirstRunOutcome::Seeded
    );
    assert_eq!(
        contents(&mut store, &first),
        FirstRunEnvironment::Standalone
            .thought_contents()
            .map(str::to_owned)
    );
    assert_eq!(
        store
            .search_sessions(&SessionQuery {
                text: Some("herdr.dev".to_owned()),
                ..SessionQuery::default()
            })
            .expect("URL search")[0]
            .id,
        first.board().session.id
    );

    assert_eq!(
        store
            .create_first_run_session(&second)
            .expect("later session"),
        FirstRunOutcome::AlreadyCompleted
    );
    assert!(contents(&mut store, &second).is_empty());
}

#[test]
fn select_all_delete_is_one_ordinary_operation_and_one_undo_restores_all_six() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let candidate = candidate(
        &mut ids,
        &test_path("proqi-onboarding-undo"),
        FirstRunEnvironment::Standalone,
    );
    store
        .create_first_run_session(&candidate)
        .expect("seed practice board");
    let session_id = candidate.board().session.id;
    let mut state = AppState::from_snapshot(store.load_session(session_id).expect("snapshot"))
        .expect("application state");
    let thought_ids = state
        .board
        .live_thoughts()
        .into_iter()
        .map(|thought| thought.id)
        .collect();
    let delete = one_effect(
        &mut state,
        Action::DeleteThoughts {
            operation_id: ids.operation_id(),
            thought_ids,
            kind: BoardOperationKind::Delete,
            at: Timestamp::from_millis(2),
        },
    );
    persist_effect(&mut store, &delete);
    assert!(contents(&mut store, &candidate).is_empty());
    let deleted = store.load_session(session_id).expect("deleted snapshot");
    assert_eq!(deleted.board_operations.len(), 1);
    assert_eq!(deleted.board_history_cursor, 1);

    let undo = one_effect(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &undo);
    assert_eq!(contents(&mut store, &candidate).len(), 6);
}

#[test]
fn simultaneous_claims_seed_at_most_one_complete_board() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let candidates = [
        candidate(
            &mut ids,
            &test_path("proqi-onboarding-race-one"),
            FirstRunEnvironment::Standalone,
        ),
        candidate(
            &mut ids,
            &test_path("proqi-onboarding-race-two"),
            FirstRunEnvironment::HerdrManaged,
        ),
    ];
    let barrier = Arc::new(Barrier::new(2));
    let outcomes = std::thread::scope(|scope| {
        let handles = candidates
            .iter()
            .map(|candidate| {
                let config = fixture.config.clone();
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    let mut store = SqliteStore::open(&config).expect("competing store");
                    barrier.wait();
                    store
                        .create_first_run_session(candidate)
                        .expect("competing claim")
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("claim thread"))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| **outcome == FirstRunOutcome::Seeded)
            .count(),
        1
    );
    let connection = Connection::open(&fixture.config.database_path).expect("verification DB");
    let (sessions, thoughts, completed): (i64, i64, i64) = connection
        .query_row(
            "SELECT (SELECT count(*) FROM sessions),
                    (SELECT count(*) FROM thoughts),
                    (SELECT completed_version FROM onboarding_state WHERE singleton = 1)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("durable race result");
    assert_eq!((sessions, thoughts, completed), (2, 6, 1));
}

#[test]
fn injected_insert_failure_rolls_back_marker_session_thoughts_and_search() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let candidate = candidate(
        &mut ids,
        &test_path("proqi-onboarding-rollback"),
        FirstRunEnvironment::Standalone,
    );
    let connection = Connection::open(&fixture.config.database_path).expect("failure injector");
    connection
        .execute_batch(
            "CREATE TRIGGER fail_practice_insert BEFORE INSERT ON thoughts
             WHEN NEW.position = 2 BEGIN
                 SELECT RAISE(ABORT, 'injected practice insert failure');
             END;",
        )
        .expect("failure trigger");
    assert!(store.create_first_run_session(&candidate).is_err());
    let counts: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT (SELECT completed_version FROM onboarding_state WHERE singleton = 1),
                    (SELECT count(*) FROM sessions),
                    (SELECT count(*) FROM thoughts),
                    (SELECT count(*) FROM session_search)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("rolled back counts");
    assert_eq!(counts, (0, 0, 0, 0));

    connection
        .execute_batch("DROP TRIGGER fail_practice_insert")
        .expect("remove failure trigger");
    assert_eq!(
        store
            .create_first_run_session(&candidate)
            .expect("retry claim"),
        FirstRunOutcome::Seeded
    );
    assert_eq!(contents(&mut store, &candidate).len(), 6);
}
