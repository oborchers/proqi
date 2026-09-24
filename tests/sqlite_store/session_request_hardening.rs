//! Store-side validation of named creation and retired history identities.

use std::path::PathBuf;

use proqi::{
    adapters::memory::FakeIdGenerator,
    domain::{BrowserOperation, BrowserOperationKind, Session, SessionId, Timestamp},
    ports::{
        environment::IdGenerator,
        store::{
            BrowserHistoryEntry, NamedSessionCreation, NamedSessionPolicy, OperationBatch,
            SessionRequest, Store, StoreError, StoredOperationRequest, StoredSessionRequest,
        },
    },
};

use super::DatabaseFixture;

fn named(id: SessionId, name: &str) -> Session {
    Session::with_name(
        id,
        PathBuf::from("/work"),
        Timestamp::from_millis(5),
        Some(name.to_owned()),
    )
    .expect("named session")
}

#[test]
fn creation_rejects_foreign_identities_trashed_sessions_and_reserved_get_or_create() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_500_000_000);
    let operation_id = ids.operation_id();
    let derived = SessionId::from_database_bytes(operation_id.database_bytes()).expect("derived");

    let foreign = NamedSessionCreation {
        session: named(ids.session_id(), "foreign"),
        policy: NamedSessionPolicy::Always,
        operation_id: Some(operation_id),
    };
    let mut trashed = named(derived, "trashed");
    trashed.deleted_at = Some(Timestamp::from_millis(6));
    let trashed = NamedSessionCreation {
        session: trashed,
        policy: NamedSessionPolicy::Always,
        operation_id: Some(operation_id),
    };
    let reserved_ensure = NamedSessionCreation {
        session: named(derived, "ensure"),
        policy: NamedSessionPolicy::UnlessNameExists,
        operation_id: Some(operation_id),
    };
    for creation in [foreign, trashed, reserved_ensure] {
        assert!(
            matches!(
                store.create_named_session(&creation),
                Err(StoreError::Invariant(_))
            ),
            "{creation:?}"
        );
    }
    assert_eq!(store.session_request(operation_id), Ok(None));
    assert!(matches!(
        store.load_session(derived),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn a_pruned_history_target_stays_reserved_and_its_undo_replays_deterministically() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_500_100_000);
    let session_id = ids.session_id();
    store
        .commit(&OperationBatch::CreateSession(named(session_id, "gone")))
        .expect("create");
    let trash = BrowserOperation::trash(
        ids.operation_id(),
        session_id,
        Timestamp::from_millis(5),
        Timestamp::from_millis(6),
    );
    store.commit_browser_operation(&trash).expect("trash");
    let undo = ids.operation_id();
    let entry = BrowserHistoryEntry {
        operation_id: trash.id(),
        session_id,
        kind: BrowserOperationKind::Trash,
    };
    store
        .move_browser_history(undo, entry, true, Timestamp::from_millis(7))
        .expect("undo trash");
    store
        .trash_session(session_id, Timestamp::from_millis(8))
        .expect("trash again");
    store
        .prune_session_request(session_id, ids.operation_id(), Timestamp::from_millis(9))
        .expect("prune");

    assert!(matches!(
        store.session_request(undo),
        Ok(Some(StoredSessionRequest::Administration(receipt)))
            if receipt.request == SessionRequest::History { undo: true }
                && receipt.history_target.is_none()
    ));
    assert_eq!(
        store.session_request(trash.id()),
        Ok(Some(StoredSessionRequest::RetiredHistoryTarget))
    );
    assert_eq!(
        store.operation_request(trash.id()),
        Ok(Some(StoredOperationRequest::SessionAdministration))
    );

    let other = ids.session_id();
    store
        .commit(&OperationBatch::CreateSession(named(other, "other")))
        .expect("other session");
    let reuse = BrowserOperation::rename(
        trash.id(),
        other,
        Some("other".to_owned()),
        Some("reused".to_owned()),
        Timestamp::from_millis(10),
    )
    .expect("rename");
    assert!(matches!(
        store.commit_browser_operation(&reuse),
        Err(StoreError::Conflict(_))
    ));
    assert_eq!(
        store
            .load_session(other)
            .expect("unchanged")
            .board
            .session
            .name
            .as_deref(),
        Some("other")
    );
}

#[test]
fn a_session_identity_is_visible_to_thought_operation_lookups() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_500_200_000);
    let session_id = ids.session_id();
    store
        .commit(&OperationBatch::CreateSession(named(session_id, "kept")))
        .expect("create");
    let rename = ids.operation_id();
    store
        .commit_browser_noop_rename(rename, session_id, Some("kept"), Timestamp::from_millis(6))
        .expect("rename receipt");
    assert_eq!(
        store.operation_request(rename),
        Ok(Some(StoredOperationRequest::SessionAdministration))
    );
    assert_eq!(store.operation_request(ids.operation_id()), Ok(None));
}
