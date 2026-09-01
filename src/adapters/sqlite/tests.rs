//! Deterministic transaction-boundary contention tests.

use std::time::Duration;

use tempfile::tempdir;

use crate::{
    adapters::memory::FakeIdGenerator,
    domain::{Session, Timestamp},
    ports::{
        environment::IdGenerator,
        store::{MigrationMode, OperationBatch, StoreError},
    },
};

use super::{RetryPolicy, SqliteStore, StoreConfig, board_commit::commit_batch};

#[test]
fn transient_busy_retries_whole_session_transaction_without_duplication() {
    let temporary = tempdir().expect("temporary state root");
    let mut config = StoreConfig::new(
        temporary.path().join("data/proqi.sqlite3"),
        temporary.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    config.retry = RetryPolicy {
        busy_timeout: Duration::from_millis(1),
        max_attempts: 2,
        base_delay: Duration::ZERO,
        jitter_seed: 1,
    };
    let mut store = SqliteStore::open(&config).expect("open store");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session = Session::new(
        ids.session_id(),
        temporary.path().to_path_buf(),
        Timestamp::from_millis(2),
    )
    .expect("session");
    let session_id = session.id;
    let batch = OperationBatch::CreateSession(session);
    let mut attempts = 0;

    let receipt = store.with_write_retry(|transaction| {
        attempts += 1;
        let receipt = commit_batch(transaction, &batch)?;
        if attempts == 1 {
            return Err(StoreError::Busy);
        }
        Ok(receipt)
    });

    assert_eq!(receipt, Ok(None));
    assert_eq!(attempts, 2);
    let session_rows: i64 = store
        .connection
        .query_row("SELECT count(*) FROM sessions", [], |row| row.get(0))
        .expect("session rows");
    let search_rows: i64 = store
        .connection
        .query_row(
            "SELECT count(*) FROM session_search WHERE session_id = ?1",
            [session_id.to_string()],
            |row| row.get(0),
        )
        .expect("search rows");
    let total_search_rows: i64 = store
        .connection
        .query_row("SELECT count(*) FROM session_search", [], |row| row.get(0))
        .expect("total search rows");
    assert_eq!(session_rows, 1);
    assert_eq!(search_rows, 1);
    assert_eq!(total_search_rows, 1);
    for table in [
        "thoughts",
        "board_operations",
        "thought_revisions",
        "commit_receipts",
    ] {
        let rows: i64 = store
            .connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("fresh-session side table");
        assert_eq!(rows, 0, "fresh session unexpectedly populated {table}");
    }
}

#[test]
fn persistent_busy_stops_after_production_attempt_bound_without_partial_state() {
    let temporary = tempdir().expect("temporary state root");
    let mut config = StoreConfig::new(
        temporary.path().join("data/proqi.sqlite3"),
        temporary.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    config.retry.jitter_seed = 1;
    let expected_attempts = config.retry.max_attempts;
    let mut store = SqliteStore::open(&config).expect("open store");
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let session = Session::new(
        ids.session_id(),
        temporary.path().to_path_buf(),
        Timestamp::from_millis(2),
    )
    .expect("session");
    let batch = OperationBatch::CreateSession(session);
    let mut attempts = 0;

    let receipt = store.with_write_retry(|transaction| {
        attempts += 1;
        let _receipt = commit_batch(transaction, &batch)?;
        Err::<Option<crate::ports::store::CommitReceipt>, _>(StoreError::Busy)
    });

    assert_eq!(receipt, Err(StoreError::Busy));
    assert_eq!(attempts, expected_attempts);
    for table in ["sessions", "session_search"] {
        let rows: i64 = store
            .connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("persistent-busy table");
        assert_eq!(rows, 0, "persistent busy left partial state in {table}");
    }
}
