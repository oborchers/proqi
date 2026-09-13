//! Durable installation-wide Browser history contracts.

use super::*;

#[path = "browser_history/activity.rs"]
mod activity;
#[path = "browser_history/identity.rs"]
mod identity;

fn create_session(
    store: &mut SqliteStore,
    ids: &mut FakeIdGenerator,
    name: &str,
) -> proqi::domain::SessionId {
    let mut session = Session::new(
        ids.session_id(),
        test_path("proqi-browser-history"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    session.name = Some(name.to_owned());
    let id = session.id;
    store
        .commit(&OperationBatch::CreateSession(session))
        .expect("create session");
    id
}

fn rename(
    ids: &mut FakeIdGenerator,
    session_id: proqi::domain::SessionId,
    before: &str,
    after: &str,
) -> BrowserOperation {
    BrowserOperation::rename(
        ids.operation_id(),
        session_id,
        Some(before.to_owned()),
        Some(after.to_owned()),
        Timestamp::from_millis(2),
    )
    .expect("rename operation")
}

fn history_entry(operation: &BrowserOperation) -> proqi::ports::store::BrowserHistoryEntry {
    proqi::ports::store::BrowserHistoryEntry {
        operation_id: operation.id(),
        session_id: operation.session_id(),
        kind: operation.kind(),
    }
}

fn move_history(
    store: &mut SqliteStore,
    request_id: proqi::domain::OperationId,
    undo: bool,
    at: Timestamp,
) -> Result<proqi::ports::store::BrowserCommitReceipt, StoreError> {
    let status = store.browser_history_status()?;
    let target = if undo { status.undo } else { status.redo }
        .ok_or_else(|| StoreError::Conflict("missing test history target".to_owned()))?;
    store.move_browser_history(request_id, target, undo, at)
}

#[test]
fn browser_history_survives_restart_and_spans_sessions() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let first = create_session(&mut store, &mut ids, "first");
    let second = create_session(&mut store, &mut ids, "second");
    store
        .commit_browser_operation(&rename(&mut ids, first, "first", "one"))
        .expect("rename first");
    store
        .commit_browser_operation(&rename(&mut ids, second, "second", "two"))
        .expect("rename second");
    assert_eq!(
        store
            .browser_history_status()
            .expect("status")
            .undo
            .map(|entry| entry.kind),
        Some(BrowserOperationKind::Rename)
    );
    drop(store);

    let mut reopened = fixture.open();
    move_history(
        &mut reopened,
        ids.operation_id(),
        true,
        Timestamp::from_millis(3),
    )
    .expect("undo second rename");
    assert_eq!(
        reopened
            .load_session(second)
            .expect("second")
            .board
            .session
            .name
            .as_deref(),
        Some("second")
    );
    assert_eq!(
        reopened
            .load_session(first)
            .expect("first")
            .board
            .session
            .name
            .as_deref(),
        Some("one")
    );
    move_history(
        &mut reopened,
        ids.operation_id(),
        false,
        Timestamp::from_millis(4),
    )
    .expect("redo second rename");
    assert_eq!(
        reopened
            .load_session(second)
            .expect("second")
            .board
            .session
            .name
            .as_deref(),
        Some("two")
    );
}

#[test]
fn divergent_browser_mutation_invalidates_redo_without_touching_session_history() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = create_session(&mut store, &mut ids, "first");
    let original = rename(&mut ids, session_id, "first", "second");
    store.commit_browser_operation(&original).expect("rename");
    move_history(
        &mut store,
        ids.operation_id(),
        true,
        Timestamp::from_millis(3),
    )
    .expect("undo");
    store
        .commit_browser_operation(&rename(&mut ids, session_id, "first", "third"))
        .expect("divergent rename");
    let status = store.browser_history_status().expect("status");
    assert_eq!(
        status.undo.map(|entry| entry.kind),
        Some(BrowserOperationKind::Rename)
    );
    assert_eq!(status.redo, None);
    assert!(matches!(
        store.move_browser_history(
            ids.operation_id(),
            history_entry(&original),
            false,
            Timestamp::from_millis(4)
        ),
        Err(StoreError::Conflict(message)) if message == "nothing to redo in Browser history"
    ));
    let replay = store
        .commit_browser_operation(&original)
        .expect("retained idempotent replay");
    assert!(replay.idempotent_replay);
    assert_eq!(
        store
            .load_session(session_id)
            .expect("unchanged divergent session")
            .board
            .session
            .name
            .as_deref(),
        Some("third")
    );
    assert_eq!(
        store.browser_operation(original.id()).expect("lookup"),
        Some(original)
    );
}

#[test]
fn browser_history_compare_and_set_rejects_external_drift() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = create_session(&mut store, &mut ids, "first");
    let operation = rename(&mut ids, session_id, "first", "second");
    store
        .rename_session(session_id, Some("external"))
        .expect("external drift");
    assert!(matches!(
        store.commit_browser_operation(&operation),
        Err(StoreError::Conflict(message))
            if message == "session name changed before Browser history commit"
    ));
    assert_eq!(
        store.browser_history_status().expect("status"),
        proqi::ports::store::BrowserHistoryStatus::default()
    );
}

#[test]
fn browser_history_move_rejects_a_stale_presented_target() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = create_session(&mut store, &mut ids, "first");
    let first = rename(&mut ids, session_id, "first", "second");
    let second = rename(&mut ids, session_id, "second", "third");
    store
        .commit_browser_operation(&first)
        .expect("first rename");
    store
        .commit_browser_operation(&second)
        .expect("second rename");

    assert!(matches!(
        store.move_browser_history(
            ids.operation_id(),
            history_entry(&first),
            true,
            Timestamp::from_millis(3),
        ),
        Err(StoreError::Conflict(message))
            if message == "Browser history changed before the requested movement"
    ));
    assert_eq!(
        store
            .load_session(session_id)
            .expect("unchanged session")
            .board
            .session
            .name
            .as_deref(),
        Some("third")
    );
    assert_eq!(
        store
            .browser_history_status()
            .expect("unchanged cursor")
            .undo,
        Some(history_entry(&second))
    );
}

#[test]
fn trash_history_restores_exact_activity_time_across_restart() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = create_session(&mut store, &mut ids, "first");
    let operation = BrowserOperation::trash(
        ids.operation_id(),
        session_id,
        Timestamp::from_millis(1),
        Timestamp::from_millis(10),
    );
    store
        .commit_browser_operation(&operation)
        .expect("trash operation");
    assert_eq!(
        store
            .load_session(session_id)
            .expect("trashed")
            .board
            .session
            .last_active_at,
        Timestamp::from_millis(10)
    );
    drop(store);

    let mut reopened = fixture.open();
    move_history(
        &mut reopened,
        ids.operation_id(),
        true,
        Timestamp::from_millis(100),
    )
    .expect("undo trash");
    let restored = reopened.load_session(session_id).expect("restored");
    assert_eq!(restored.board.session.deleted_at, None);
    assert_eq!(
        restored.board.session.last_active_at,
        Timestamp::from_millis(1)
    );
    move_history(
        &mut reopened,
        ids.operation_id(),
        false,
        Timestamp::from_millis(200),
    )
    .expect("redo trash");
    let trashed = reopened.load_session(session_id).expect("trashed again");
    assert_eq!(
        trashed.board.session.deleted_at,
        Some(Timestamp::from_millis(10))
    );
    assert_eq!(
        trashed.board.session.last_active_at,
        Timestamp::from_millis(10)
    );
}

#[test]
fn opening_a_restored_session_invalidates_only_its_conflicting_redo() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let restored = create_session(&mut store, &mut ids, "restored");
    let unrelated = create_session(&mut store, &mut ids, "unrelated");
    let unrelated_rename = rename(&mut ids, unrelated, "unrelated", "renamed");
    store
        .commit_browser_operation(&unrelated_rename)
        .expect("unrelated rename");
    let trash = BrowserOperation::trash(
        ids.operation_id(),
        restored,
        Timestamp::from_millis(1),
        Timestamp::from_millis(10),
    );
    store.commit_browser_operation(&trash).expect("trash");
    move_history(
        &mut store,
        ids.operation_id(),
        true,
        Timestamp::from_millis(11),
    )
    .expect("undo trash");

    store
        .record_session_open(
            unrelated,
            &test_path("unrelated-open"),
            Timestamp::from_millis(20),
        )
        .expect("open unrelated session");
    assert_eq!(
        store.browser_history_status().expect("redo remains").redo,
        Some(history_entry(&trash))
    );

    store
        .record_session_open(
            restored,
            &test_path("restored-open"),
            Timestamp::from_millis(21),
        )
        .expect("open restored session");
    assert_eq!(
        store
            .browser_history_status()
            .expect("redo invalidated")
            .redo,
        None
    );
    assert_eq!(
        store
            .load_session(restored)
            .expect("restored remains live")
            .board
            .session
            .deleted_at,
        None
    );
}

#[test]
fn pruning_one_session_preserves_unrelated_browser_history() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let first = create_session(&mut store, &mut ids, "first");
    let second = create_session(&mut store, &mut ids, "second");
    let first_rename = rename(&mut ids, first, "first", "one");
    let second_rename = rename(&mut ids, second, "second", "two");
    store
        .commit_browser_operation(&first_rename)
        .expect("rename first");
    store
        .commit_browser_operation(&second_rename)
        .expect("rename second");
    let trash = BrowserOperation::trash(
        ids.operation_id(),
        first,
        Timestamp::from_millis(1),
        Timestamp::from_millis(10),
    );
    store.commit_browser_operation(&trash).expect("trash first");
    store.prune_session(first).expect("prune first");

    assert_eq!(
        store.browser_operation(first_rename.id()).expect("lookup"),
        None
    );
    assert_eq!(
        store
            .browser_operation(second_rename.id())
            .expect("unrelated lookup"),
        Some(second_rename)
    );
    assert_eq!(
        store
            .browser_history_status()
            .expect("status")
            .undo
            .map(|entry| entry.kind),
        Some(BrowserOperationKind::Rename)
    );
    move_history(
        &mut store,
        ids.operation_id(),
        true,
        Timestamp::from_millis(11),
    )
    .expect("undo remaining rename");
    assert_eq!(
        store
            .load_session(second)
            .expect("second")
            .board
            .session
            .name
            .as_deref(),
        Some("second")
    );
}

#[test]
fn browser_busy_failure_is_atomic_and_exact_retry_is_idempotent() {
    let fixture = DatabaseFixture::new();
    let mut setup = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session_id = create_session(&mut setup, &mut ids, "first");
    drop(setup);
    let mut config = fixture.config.clone();
    config.retry = RetryPolicy {
        busy_timeout: Duration::from_millis(1),
        max_attempts: 2,
        base_delay: Duration::ZERO,
        jitter_seed: 1,
    };
    let mut store = SqliteStore::open(&config).expect("store");
    let operation = rename(&mut ids, session_id, "first", "second");
    let raw = Connection::open(&config.database_path).expect("contending connection");
    raw.execute_batch("BEGIN IMMEDIATE").expect("writer lock");

    assert_eq!(
        store.commit_browser_operation(&operation),
        Err(StoreError::Busy)
    );
    assert_eq!(
        store
            .load_session(session_id)
            .expect("unchanged")
            .board
            .session
            .name
            .as_deref(),
        Some("first")
    );
    assert_eq!(
        store.browser_history_status().expect("empty history"),
        proqi::ports::store::BrowserHistoryStatus::default()
    );
    raw.execute_batch("ROLLBACK").expect("release writer");

    let receipt = store
        .commit_browser_operation(&operation)
        .expect("exact retry");
    assert!(!receipt.idempotent_replay);
    let replay = store
        .commit_browser_operation(&operation)
        .expect("idempotent replay");
    assert!(replay.idempotent_replay);
    assert_eq!(receipt.cursor, replay.cursor);
}
