//! Atomic named creation and retained session-administration request receipts.

use std::{
    path::PathBuf,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

use proqi::{
    adapters::{
        memory::FakeIdGenerator,
        sqlite::{RetryPolicy, SqliteStore},
    },
    domain::{BrowserOperation, BrowserOperationKind, OperationId, Session, Timestamp},
    ports::{
        environment::IdGenerator,
        store::{
            NamedSessionCreation, NamedSessionOutcome, NamedSessionPolicy, OperationBatch,
            SessionRequest, Store, StoreError, StoredSessionRequest,
        },
    },
};
use rusqlite::Connection;

use super::DatabaseFixture;

fn named(ids: &mut FakeIdGenerator, name: &str, cwd: &str) -> Session {
    Session::with_name(
        ids.session_id(),
        PathBuf::from(cwd),
        Timestamp::from_millis(5),
        Some(name.to_owned()),
    )
    .expect("named session")
}

fn ensure(session: Session) -> NamedSessionCreation {
    NamedSessionCreation {
        session,
        policy: NamedSessionPolicy::UnlessNameExists,
        operation_id: None,
    }
}

fn named_rows(fixture: &DatabaseFixture, name: &str) -> Vec<(Vec<u8>, Option<String>)> {
    let connection = Connection::open(&fixture.config.database_path).expect("inspect database");
    let mut statement = connection
        .prepare("SELECT id, name FROM sessions WHERE name = ?1 OR name IS NULL")
        .expect("prepare");
    statement
        .query_map([name], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows")
}

#[test]
fn concurrent_named_get_or_create_inserts_exactly_one_session() {
    let fixture = Arc::new(DatabaseFixture::new());
    drop(fixture.open());
    let contenders = 12;
    let barrier = Arc::new(Barrier::new(contenders));
    let workers = (0..contenders)
        .map(|index| {
            let fixture = Arc::clone(&fixture);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let offset = u64::try_from(index).expect("worker index") * 1_000;
                let mut ids = FakeIdGenerator::new(1_725_400_000_000 + offset);
                let mut store = fixture.open();
                let session = named(&mut ids, "agent-os-claude", "/work/agent-os");
                let candidate = session.id;
                barrier.wait();
                (
                    candidate,
                    store
                        .create_named_session(&ensure(session))
                        .expect("atomic creation"),
                )
            })
        })
        .collect::<Vec<_>>();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .collect::<Vec<_>>();

    let created = results
        .iter()
        .filter(|(_, outcome)| *outcome == NamedSessionOutcome::Created)
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    assert_eq!(created.len(), 1, "{results:?}");
    for (_, outcome) in &results {
        if let NamedSessionOutcome::NameInUse(existing) = outcome {
            assert_eq!(existing.len(), 1);
            assert_eq!(existing[0].id, created[0]);
            assert_eq!(existing[0].origin_cwd, PathBuf::from("/work/agent-os"));
        }
    }
    let rows = named_rows(&fixture, "agent-os-claude");
    assert_eq!(rows.len(), 1, "no duplicate or unnamed session may remain");
    assert_eq!(rows[0].1.as_deref(), Some("agent-os-claude"));
}

#[test]
fn name_collision_policy_ignores_trashed_sessions_and_reports_every_live_owner() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_400_100_000);
    let trashed = named(&mut ids, "agent", "/work/a");
    let trashed_id = trashed.id;
    assert_eq!(
        store.create_named_session(&ensure(trashed)),
        Ok(NamedSessionOutcome::Created)
    );
    store
        .trash_session(trashed_id, Timestamp::from_millis(6))
        .expect("trash");
    let replacement = named(&mut ids, "agent", "/work/a");
    let replacement_id = replacement.id;
    assert_eq!(
        store.create_named_session(&ensure(replacement)),
        Ok(NamedSessionOutcome::Created)
    );
    let duplicate = named(&mut ids, "agent", "/work/b");
    let duplicate_id = duplicate.id;
    assert_eq!(
        store.create_named_session(&NamedSessionCreation {
            session: duplicate,
            policy: NamedSessionPolicy::Always,
            operation_id: None,
        }),
        Ok(NamedSessionOutcome::Created)
    );

    let NamedSessionOutcome::NameInUse(existing) = store
        .create_named_session(&ensure(named(&mut ids, "agent", "/work/c")))
        .expect("observation")
    else {
        panic!("live name must be reported");
    };
    let mut existing_ids = existing.iter().map(|entry| entry.id).collect::<Vec<_>>();
    existing_ids.sort();
    let mut expected = vec![replacement_id, duplicate_id];
    expected.sort();
    assert_eq!(existing_ids, expected);
}

#[test]
fn unconditional_creation_replays_its_identity_and_rejects_divergent_reuse() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_400_200_000);
    let operation_id = ids.operation_id();
    let session_id = operation_id_session(operation_id);
    let request = |name: &str, cwd: &str| NamedSessionCreation {
        session: Session::with_name(
            session_id,
            PathBuf::from(cwd),
            Timestamp::from_millis(9),
            Some(name.to_owned()),
        )
        .expect("session"),
        policy: NamedSessionPolicy::Always,
        operation_id: Some(operation_id),
    };

    assert_eq!(
        store.create_named_session(&request("scratch", "/work")),
        Ok(NamedSessionOutcome::Created)
    );
    assert_eq!(
        store.create_named_session(&request("scratch", "/work")),
        Ok(NamedSessionOutcome::Replayed)
    );
    assert_eq!(
        store.create_named_session(&request("other", "/work")),
        Ok(NamedSessionOutcome::IdentityReused)
    );
    assert_eq!(
        store.create_named_session(&request("scratch", "/elsewhere")),
        Ok(NamedSessionOutcome::IdentityReused)
    );
    assert!(matches!(
        store.session_request(operation_id),
        Ok(Some(StoredSessionRequest::Administration(receipt)))
            if receipt.request == SessionRequest::create(
                session_id,
                "scratch",
                std::path::Path::new("/work"),
            )
    ));
    let connection = Connection::open(&fixture.config.database_path).expect("inspect database");
    let payload: String = connection
        .query_row(
            "SELECT payload_json FROM browser_operation_receipts WHERE id = ?1",
            [operation_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("creation receipt");
    assert!(!payload.contains("scratch"), "the receipt retains no name");
    assert!(
        !payload.contains("work"),
        "the receipt retains no directory"
    );
    assert_eq!(named_rows(&fixture, "scratch").len(), 1);
}

#[test]
fn contended_named_creation_fails_without_any_partial_session() {
    let fixture = DatabaseFixture::new();
    drop(fixture.open());
    let mut config = fixture.config.clone();
    config.retry = RetryPolicy {
        busy_timeout: Duration::from_millis(1),
        max_attempts: 2,
        base_delay: Duration::ZERO,
        jitter_seed: 1,
    };
    let mut store = SqliteStore::open(&config).expect("store");
    let mut ids = FakeIdGenerator::new(1_725_400_300_000);
    let raw = Connection::open(&config.database_path).expect("contending connection");
    raw.execute_batch("BEGIN IMMEDIATE").expect("writer lock");

    assert_eq!(
        store.create_named_session(&ensure(named(&mut ids, "blocked", "/work"))),
        Err(StoreError::Busy)
    );
    raw.execute_batch("ROLLBACK").expect("release writer");
    assert!(named_rows(&fixture, "blocked").is_empty());
}

#[test]
fn every_session_request_kind_is_retained_under_its_identity() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_400_400_000);
    let session = named(&mut ids, "kept", "/work");
    let session_id = session.id;
    store
        .commit(&OperationBatch::CreateSession(session))
        .expect("create");

    let noop_rename = ids.operation_id();
    store
        .commit_browser_noop_rename(
            noop_rename,
            session_id,
            Some("kept"),
            Timestamp::from_millis(6),
        )
        .expect("no-op rename");
    let trash = BrowserOperation::trash(
        ids.operation_id(),
        session_id,
        Timestamp::from_millis(5),
        Timestamp::from_millis(7),
    );
    store.commit_browser_operation(&trash).expect("trash");
    let noop_trash = ids.operation_id();
    let first = store
        .commit_noop_trash(noop_trash, session_id, Timestamp::from_millis(8))
        .expect("no-op trash");
    let replay = store
        .commit_noop_trash(noop_trash, session_id, Timestamp::from_millis(9))
        .expect("replay");
    assert!(!first.idempotent_replay);
    assert!(replay.idempotent_replay);
    let history = ids.operation_id();
    let entry = proqi::ports::store::BrowserHistoryEntry {
        operation_id: trash.id(),
        session_id,
        kind: BrowserOperationKind::Trash,
    };
    store
        .move_browser_history(history, entry, true, Timestamp::from_millis(10))
        .expect("undo trash");

    let request = |store: &mut SqliteStore, operation_id| match store
        .session_request(operation_id)
        .expect("lookup")
    {
        Some(StoredSessionRequest::Administration(receipt)) => {
            (receipt.request, receipt.history_target)
        }
        other => panic!("unexpected request: {other:?}"),
    };
    assert_eq!(
        request(&mut store, noop_rename).0,
        SessionRequest::Rename {
            session_id,
            name: Some("kept".to_owned()),
        }
    );
    assert_eq!(
        request(&mut store, trash.id()).0,
        SessionRequest::Trash { session_id }
    );
    assert_eq!(
        request(&mut store, noop_trash).0,
        SessionRequest::Trash { session_id }
    );
    assert_eq!(
        request(&mut store, history),
        (
            SessionRequest::History { undo: true },
            Some(BrowserOperationKind::Trash)
        )
    );
    assert_eq!(store.session_request(ids.operation_id()), Ok(None));
}

#[test]
fn trash_receipts_require_trash_and_reject_reused_identities() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_400_500_000);
    let session = named(&mut ids, "live", "/work");
    let session_id = session.id;
    store
        .commit(&OperationBatch::CreateSession(session))
        .expect("create");
    assert!(matches!(
        store.commit_noop_trash(ids.operation_id(), session_id, Timestamp::from_millis(6)),
        Err(StoreError::Conflict(_))
    ));

    let rename = ids.operation_id();
    store
        .commit_browser_noop_rename(rename, session_id, Some("live"), Timestamp::from_millis(6))
        .expect("rename receipt");
    store
        .trash_session(session_id, Timestamp::from_millis(7))
        .expect("trash");
    assert!(matches!(
        store.commit_noop_trash(rename, session_id, Timestamp::from_millis(8)),
        Err(StoreError::Conflict(_))
    ));
}

#[test]
fn prune_receipt_survives_the_pruned_session_and_replays() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_400_600_000);
    let session = named(&mut ids, "gone", "/work");
    let session_id = session.id;
    store
        .commit(&OperationBatch::CreateSession(session))
        .expect("create");
    let prune = ids.operation_id();
    assert!(matches!(
        store.prune_session_request(session_id, prune, Timestamp::from_millis(6)),
        Err(StoreError::Conflict(_))
    ));
    store
        .trash_session(session_id, Timestamp::from_millis(7))
        .expect("trash");
    let first = store
        .prune_session_request(session_id, prune, Timestamp::from_millis(8))
        .expect("prune");
    assert!(!first.idempotent_replay);
    assert!(matches!(
        store.load_session(session_id),
        Err(StoreError::NotFound(_))
    ));
    let replay = store
        .prune_session_request(session_id, prune, Timestamp::from_millis(9))
        .expect("replay");
    assert!(replay.idempotent_replay);
    assert!(matches!(
        store.session_request(prune),
        Ok(Some(StoredSessionRequest::Administration(receipt)))
            if receipt.request == SessionRequest::Prune { session_id }
    ));
}

#[test]
fn prune_retains_creation_receipts_so_a_creation_retry_cannot_resurrect() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_400_700_000);
    let operation_id = ids.operation_id();
    let session_id = operation_id_session(operation_id);
    let creation = NamedSessionCreation {
        session: Session::with_name(
            session_id,
            PathBuf::from("/work"),
            Timestamp::from_millis(5),
            Some("pruned".to_owned()),
        )
        .expect("session"),
        policy: NamedSessionPolicy::Always,
        operation_id: Some(operation_id),
    };
    assert_eq!(
        store.create_named_session(&creation),
        Ok(NamedSessionOutcome::Created)
    );
    store
        .trash_session(session_id, Timestamp::from_millis(6))
        .expect("trash");
    store
        .prune_session_request(session_id, ids.operation_id(), Timestamp::from_millis(7))
        .expect("prune");

    assert_eq!(
        store.create_named_session(&creation),
        Ok(NamedSessionOutcome::Replayed)
    );
    assert!(named_rows(&fixture, "pruned").is_empty());
    assert!(matches!(
        store.load_session(session_id),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn a_derived_identity_that_already_names_a_session_is_reused_not_overwritten() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_400_800_000);
    let operation_id = ids.operation_id();
    let session_id = operation_id_session(operation_id);
    let existing = Session::with_name(
        session_id,
        PathBuf::from("/other"),
        Timestamp::from_millis(5),
        Some("existing".to_owned()),
    )
    .expect("session");
    store
        .commit(&OperationBatch::CreateSession(existing))
        .expect("create existing session");
    let creation = NamedSessionCreation {
        session: Session::with_name(
            session_id,
            PathBuf::from("/work"),
            Timestamp::from_millis(6),
            Some("fresh".to_owned()),
        )
        .expect("session"),
        policy: NamedSessionPolicy::Always,
        operation_id: Some(operation_id),
    };

    assert_eq!(
        store.create_named_session(&creation),
        Ok(NamedSessionOutcome::IdentityReused)
    );
    assert_eq!(
        store
            .load_session(session_id)
            .expect("unchanged")
            .board
            .session
            .name
            .as_deref(),
        Some("existing")
    );
}

fn operation_id_session(operation_id: OperationId) -> proqi::domain::SessionId {
    proqi::domain::SessionId::from_database_bytes(operation_id.database_bytes())
        .expect("derived session identity")
}
