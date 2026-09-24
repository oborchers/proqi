use tempfile::tempdir;

use crate::{
    adapters::memory::FakeIdGenerator,
    domain::{OperationSequence, Session, Timestamp},
    ports::{
        environment::IdGenerator,
        store::{CommitReceipt, DurableIdentity, MigrationMode, OperationBatch, Store, StoreError},
        transfer::{SessionTransferBatchRequest, TransferItem},
    },
};

use super::super::{SqliteStore, StoreConfig};

fn request(ids: &mut FakeIdGenerator) -> SessionTransferBatchRequest {
    SessionTransferBatchRequest {
        source_session_id: ids.session_id(),
        destination_session_id: ids.session_id(),
        operation_id: ids.operation_id(),
        removal_operation_id: ids.operation_id(),
        items: (0..2)
            .map(|index| TransferItem {
                source_thought_id: ids.thought_id(),
                destination_thought_id: ids.thought_id(),
                content: format!("selected {index}"),
                annotations: Vec::new(),
                name: None,
            })
            .collect(),
        remove_source: false,
    }
}

fn create_sessions(store: &mut SqliteStore, request: &SessionTransferBatchRequest) {
    for id in [request.source_session_id, request.destination_session_id] {
        let session =
            Session::new(id, std::env::temp_dir(), Timestamp::from_millis(1)).expect("session");
        store
            .commit(&OperationBatch::CreateSession(session))
            .expect("create session");
    }
}

#[test]
fn selected_transfer_intent_survives_restart_and_exact_receipt_completes_cohort() {
    let temporary = tempdir().expect("temporary state root");
    let config = StoreConfig::new(
        temporary.path().join("data/proqi.sqlite3"),
        temporary.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let request = request(&mut ids);
    let at = Timestamp::from_millis(2);
    let mut store = SqliteStore::open(&config).expect("open");
    create_sessions(&mut store, &request);
    assert_eq!(store.prepare_transfer(&request, at), Ok(None));
    assert_eq!(store.prepare_transfer(&request, at), Ok(None));
    store
        .mark_transfer_sending(request.operation_id)
        .expect("sending");
    drop(store);

    let mut store = SqliteStore::open(&config).expect("restart");
    let pending = store
        .pending_transfers(request.source_session_id)
        .expect("pending");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].request, request);
    let wrong = CommitReceipt {
        session_id: request.source_session_id,
        sequence: OperationSequence::new(1),
        identity: DurableIdentity::Operation(request.operation_id),
        idempotent_replay: false,
    };
    assert!(matches!(
        store.accept_transfer(&request, wrong),
        Err(StoreError::Conflict(_))
    ));
    assert!(matches!(
        store.finish_transfer(&request, None, "kept"),
        Err(StoreError::Conflict(_))
    ));
    let receipt = CommitReceipt {
        session_id: request.destination_session_id,
        ..wrong
    };
    store.accept_transfer(&request, receipt).expect("accept");
    assert_eq!(store.prepare_transfer(&request, at), Ok(Some(receipt)));
    assert_eq!(store.finish_transfer(&request, None, "kept"), Ok(None));
    assert!(
        store
            .pending_transfers(request.source_session_id)
            .expect("completed")
            .is_empty()
    );
}

#[test]
fn overlapping_sources_and_changed_retry_cannot_create_a_second_cohort() {
    let temporary = tempdir().expect("temporary state root");
    let config = StoreConfig::new(
        temporary.path().join("data/proqi.sqlite3"),
        temporary.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let original = request(&mut ids);
    let mut store = SqliteStore::open(&config).expect("open");
    create_sessions(&mut store, &original);
    store
        .prepare_transfer(&original, Timestamp::from_millis(2))
        .expect("prepare");
    let mut changed = original.clone();
    changed.items[1].content.push_str(" changed");
    assert!(matches!(
        store.prepare_transfer(&changed, Timestamp::from_millis(2)),
        Err(StoreError::Conflict(_))
    ));
    let mut overlapping = request(&mut ids);
    overlapping.items[0].source_thought_id = original.items[0].source_thought_id;
    assert!(
        store
            .prepare_transfer(&overlapping, Timestamp::from_millis(2))
            .is_err()
    );
    assert_eq!(
        store
            .pending_transfers(original.source_session_id)
            .expect("pending")
            .len(),
        1
    );
}
