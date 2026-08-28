use std::time::Duration;

use crate::{
    adapters::{
        memory::FakeIdGenerator,
        sqlite::{RetryPolicy, StoreConfig},
        terminal::supervisor::ShutdownDeadline,
    },
    application::{Action, AppState, Effect, reduce},
    domain::{OperationSequence, Session, SessionBoard, Timestamp},
    ports::{
        environment::IdGenerator,
        store::{CommitReceipt, MigrationMode, OperationBatch, Store, StoreError},
    },
};

use super::{PersistenceLane, PersistenceResult};

fn assert_ordered_results(lane: &PersistenceLane, succeed: bool) {
    for expected in [OperationSequence::new(1), OperationSequence::new(2)] {
        let result = lane
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("persistence result");
        let (sequence, result) = sequenced(result);
        assert_eq!(sequence, expected);
        assert_eq!(result.is_ok(), succeed);
    }
}

#[test]
fn failed_batch_is_retained_and_retried_after_contention() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let database = directory.path().join("proqi.sqlite3");
    let mut config = StoreConfig::new(
        database.clone(),
        directory.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    config.retry = RetryPolicy {
        busy_timeout: Duration::from_millis(1),
        max_attempts: 1,
        base_delay: Duration::ZERO,
        jitter_seed: 1,
    };
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-persistence-lane"),
        Timestamp::from_millis(2),
    )
    .expect("session");
    let mut setup = crate::adapters::sqlite::SqliteStore::open(&config).expect("store");
    setup
        .commit(&OperationBatch::CreateSession(session.clone()))
        .expect("create session");
    let lane_store = crate::adapters::sqlite::SqliteStore::open(&config).expect("lane store");
    let lane = PersistenceLane::spawn(lane_store);
    let mut state = AppState::new(SessionBoard::new(session, Vec::new()).expect("board"));
    let effects = reduce(
        &mut state,
        Action::CreateThought {
            thought_id: ids.thought_id(),
            operation_id: ids.operation_id(),
            content: "retained through contention".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(3),
        },
    )
    .expect("create thought");
    let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
        panic!("expected board operation");
    };
    let sequence = operation.sequence;
    let lock = setup.acquire_test_write_lock().expect("acquire writer");
    lane.commit(OperationBatch::Board(operation.clone()))
        .expect("queue commit");
    let failed = lane
        .receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("failure result");
    let (failed_sequence, failed_result) = sequenced(failed);
    assert_eq!(failed_sequence, sequence);
    assert!(failed_result.is_err());
    lock.release().expect("release writer");

    lane.retry(sequence).expect("queue retry");
    let retried = lane
        .receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("retry result");
    let (retried_sequence, retried_result) = sequenced(retried);
    assert_eq!(retried_sequence, sequence);
    assert!(retried_result.is_ok());
    assert!(matches!(
        lane.receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("retry completion"),
        PersistenceResult::RetryFinished
    ));
    lane.stop(ShutdownDeadline::after(std::time::Duration::from_secs(1)))
        .expect("stop lane");
    let snapshot = setup
        .load_session(state.board.session.id)
        .expect("snapshot");
    assert_eq!(
        snapshot.board.live_thoughts()[0].content,
        "retained through contention"
    );
}

#[test]
fn retry_replays_every_retained_batch_in_sequence_order() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let mut config = StoreConfig::new(
        directory.path().join("proqi.sqlite3"),
        directory.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    config.retry = RetryPolicy {
        busy_timeout: Duration::from_millis(1),
        max_attempts: 1,
        base_delay: Duration::ZERO,
        jitter_seed: 1,
    };
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-persistence-order"),
        Timestamp::from_millis(2),
    )
    .expect("session");
    let mut setup = crate::adapters::sqlite::SqliteStore::open(&config).expect("store");
    setup
        .commit(&OperationBatch::CreateSession(session.clone()))
        .expect("create session");
    let lane_store = crate::adapters::sqlite::SqliteStore::open(&config).expect("lane store");
    let lane = PersistenceLane::spawn(lane_store);
    let mut state = AppState::new(SessionBoard::new(session, Vec::new()).expect("board"));
    let mut batches = Vec::new();
    for content in ["first", "second"] {
        let effects = reduce(
            &mut state,
            Action::CreateThought {
                thought_id: ids.thought_id(),
                operation_id: ids.operation_id(),
                content: content.to_owned(),
                annotations: Vec::new(),
                insertion_index: None,
                at: Timestamp::from_millis(3),
            },
        )
        .expect("create thought");
        let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
            panic!("expected board operation");
        };
        batches.push(OperationBatch::Board(operation.clone()));
    }
    let lock = setup.acquire_test_write_lock().expect("acquire writer");
    for batch in batches {
        lane.commit(batch).expect("queue commit");
    }
    assert_ordered_results(&lane, false);
    lock.release().expect("release writer");
    lane.retry(OperationSequence::new(1)).expect("queue retry");
    assert_ordered_results(&lane, true);
    assert!(matches!(
        lane.receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("retry completion"),
        PersistenceResult::RetryFinished
    ));
    lane.stop(ShutdownDeadline::after(std::time::Duration::from_secs(1)))
        .expect("stop lane");
    let snapshot = setup
        .load_session(state.board.session.id)
        .expect("snapshot");
    assert_eq!(snapshot.board.live_thoughts().len(), 2);
}

#[test]
fn integration_metadata_commits_without_a_sequence_receipt() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let database = directory.path().join("proqi.sqlite3");
    let config = StoreConfig::new(
        database,
        directory.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-metadata-lane"),
        Timestamp::from_millis(2),
    )
    .expect("session");
    let mut setup = crate::adapters::sqlite::SqliteStore::open(&config).expect("store");
    setup
        .commit(&OperationBatch::CreateSession(session.clone()))
        .expect("create session");
    let lane_store = crate::adapters::sqlite::SqliteStore::open(&config).expect("lane store");
    let lane = PersistenceLane::spawn(lane_store);
    let context = crate::domain::IntegrationContext {
        provider: "herdr".to_owned(),
        direction: crate::domain::Direction::Right,
        agent_kind: crate::ports::agent::CODEX_AGENT_KIND.to_owned(),
        agent_name: "fixture".to_owned(),
        workspace_hint: Some("w1".to_owned()),
        tab_hint: Some("w1:t1".to_owned()),
        pane_hint: Some("w1:p2".to_owned()),
        verified_at: Timestamp::from_millis(3),
    };
    lane.metadata(OperationBatch::IntegrationContext {
        session_id: session.id,
        context: Some(context.clone()),
    })
    .expect("queue metadata");
    let outcome = lane
        .receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("metadata result");
    assert!(matches!(
        outcome,
        PersistenceResult::Metadata { result: Ok(()) }
    ));
    lane.stop(ShutdownDeadline::after(std::time::Duration::from_secs(1)))
        .expect("stop lane");
    assert_eq!(
        setup
            .load_session(session.id)
            .expect("snapshot")
            .integration_context,
        Some(context)
    );
}

fn sequenced(outcome: PersistenceResult) -> (OperationSequence, Result<CommitReceipt, StoreError>) {
    let PersistenceResult::Sequenced {
        sequence, result, ..
    } = outcome
    else {
        panic!("expected sequenced persistence result");
    };
    (sequence, result)
}
