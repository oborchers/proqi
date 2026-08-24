use super::*;

#[test]
fn bounded_contention_is_visible_and_failed_commit_is_not_durable() {
    let fixture = DatabaseFixture::new();
    let mut setup = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-busy"));
    let session_id = state.board.session.id;
    setup
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    drop(setup);

    let mut config = fixture.config.clone();
    config.retry = RetryPolicy {
        busy_timeout: Duration::from_millis(1),
        max_attempts: 2,
        base_delay: Duration::ZERO,
        jitter_seed: 1,
    };
    let mut store = SqliteStore::open(&config).expect("store");
    let effect = one_effect(
        &mut state,
        Action::CreateThought {
            thought_id: ids.thought_id(),
            operation_id: ids.operation_id(),
            content: "pending".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    let raw = Connection::open(&config.database_path).expect("contending connection");
    raw.execute_batch("BEGIN IMMEDIATE").expect("writer lock");
    let Effect::CommitBoardOperation(operation) = &effect else {
        panic!("board effect")
    };
    assert_eq!(
        store.commit(&OperationBatch::Board(operation.clone())),
        Err(StoreError::Busy)
    );
    assert!(
        store
            .load_session(session_id)
            .expect("unchanged")
            .board
            .thoughts()
            .is_empty()
    );
    raw.execute_batch("ROLLBACK").expect("release writer");
    persist_effect(&mut store, &effect);
    assert_eq!(
        store
            .load_session(session_id)
            .expect("durable")
            .board
            .live_thoughts()
            .len(),
        1
    );
}

#[test]
fn rejected_operation_rolls_back_current_state_and_receipt() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-rollback"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "stable", 2);
    let invalid = BoardOperation {
        id: ids.operation_id(),
        session_id,
        sequence: OperationSequence::new(2),
        kind: BoardOperationKind::Reorder,
        forward: BoardMutation::MoveThought {
            thought_id,
            from: ThoughtPosition::new(0),
            to: ThoughtPosition::new(99),
        },
        inverse: BoardMutation::MoveThought {
            thought_id,
            from: ThoughtPosition::new(99),
            to: ThoughtPosition::new(0),
        },
        created_at: Timestamp::from_millis(3),
    };
    assert!(matches!(
        store.commit(&OperationBatch::Board(invalid)),
        Err(StoreError::Invariant(_))
    ));
    let snapshot = store.load_session(session_id).expect("unchanged");
    assert_eq!(snapshot.board.live_thoughts()[0].content, "stable");
    assert_eq!(
        snapshot.board.session.last_durable_sequence,
        OperationSequence::new(1)
    );
    assert_eq!(snapshot.board_operations.len(), 1);
}

#[test]
#[ignore = "child fixture, driven by process_crash_rolls_back_uncommitted_typing"]
fn child_process_holds_uncommitted_sqlite_write() {
    let Ok(database) = std::env::var("PROQI_TEST_CHILD_DATABASE") else {
        return;
    };
    let thought_id =
        ThoughtId::from_str(&std::env::var("PROQI_TEST_CHILD_THOUGHT").expect("child thought"))
            .expect("thought ID");
    let connection = Connection::open(database).expect("child database");
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .expect("child transaction");
    connection
        .execute(
            "UPDATE thoughts SET content = 'uncommitted crash content' WHERE id = ?1",
            [thought_id.database_bytes().as_slice()],
        )
        .expect("child update");
    println!("PROQI_SQLITE_WRITE_READY");
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn process_termination_rolls_back_uncommitted_sqlite_write() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-crash"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "durable", 2);
    drop(store);

    let executable = std::env::current_exe().expect("test executable");
    let mut child = Command::new(executable)
        .arg("--exact")
        .arg("concurrency::child_process_holds_uncommitted_sqlite_write")
        .arg("--ignored")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("PROQI_TEST_CHILD_DATABASE", &fixture.config.database_path)
        .env("PROQI_TEST_CHILD_THOUGHT", thought_id.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child");
    let mut reader = BufReader::new(child.stdout.take().expect("child stdout"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).expect("child output");
        assert!(read > 0, "child exited before beginning its transaction");
        if line.contains("PROQI_SQLITE_WRITE_READY") {
            break;
        }
        assert!(Instant::now() < deadline, "child transaction timed out");
    }
    child.kill().expect("terminate child");
    child.wait().expect("reap child");

    let mut reopened = fixture.open();
    reopened.quick_check().expect("integrity after crash");
    assert_eq!(
        reopened
            .load_session(session_id)
            .expect("crash recovery")
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "durable"
    );
}

#[test]
fn load_rejects_noncontiguous_commit_history() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-corrupt-history"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    create_thought(&mut store, &mut state, &mut ids, "durable", 2);
    drop(store);
    Connection::open(&fixture.config.database_path)
        .expect("raw database")
        .execute("UPDATE commit_receipts SET sequence = 2", [])
        .expect("corruption fixture");
    let mut reopened = fixture.open();
    assert!(matches!(
        reopened.load_session(session_id),
        Err(StoreError::Corrupt(_))
    ));
}

#[test]
fn competing_connections_commit_different_sessions() {
    let fixture = DatabaseFixture::new();
    let mut setup = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut first = session_state(&mut ids, &test_path("proqi-writer-one"));
    let mut second = session_state(&mut ids, &test_path("proqi-writer-two"));
    for state in [&first, &second] {
        setup
            .commit(&OperationBatch::CreateSession(state.board.session.clone()))
            .expect("session");
    }
    let first_effect = one_effect(
        &mut first,
        Action::CreateThought {
            thought_id: ids.thought_id(),
            operation_id: ids.operation_id(),
            content: "writer one".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    let second_effect = one_effect(
        &mut second,
        Action::CreateThought {
            thought_id: ids.thought_id(),
            operation_id: ids.operation_id(),
            content: "writer two".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    drop(setup);

    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [first_effect, second_effect]
        .into_iter()
        .map(|effect| {
            let config = fixture.config.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let mut store = SqliteStore::open(&config).expect("thread store");
                barrier.wait();
                persist_effect(&mut store, &effect)
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("writer thread");
    }
    let mut verify = fixture.open();
    assert_eq!(
        verify
            .load_session(first.board.session.id)
            .expect("first")
            .board
            .live_thoughts()
            .len(),
        1
    );
    assert_eq!(
        verify
            .load_session(second.board.session.id)
            .expect("second")
            .board
            .live_thoughts()
            .len(),
        1
    );
}
