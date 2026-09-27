//! Real destination and source transactions for one selected transfer.

use tempfile::{TempDir, tempdir};

use crate::{
    adapters::memory::FakeIdGenerator,
    application::{Action, AppState, Effect, reduce},
    domain::{
        BoardOperation, BoardOperationKind, Session, SessionBoard, ThoughtId, ThoughtName,
        Timestamp,
    },
    ports::{
        environment::IdGenerator,
        store::{MigrationMode, OperationBatch, Store},
        transfer::{SessionTransferBatchRequest, TransferItem},
    },
};

use super::super::{SqliteStore, StoreConfig};

fn board_operation(state: &mut AppState, action: Action) -> BoardOperation {
    let effects = reduce(state, action).expect("reduce");
    let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
        panic!("expected one Board operation");
    };
    operation.clone()
}

fn commit(store: &mut SqliteStore, operation: BoardOperation) {
    store
        .commit(&OperationBatch::Board {
            operation,
            semantic_fingerprint: None,
        })
        .expect("commit")
        .expect("receipt");
}

fn setup() -> (
    TempDir,
    StoreConfig,
    FakeIdGenerator,
    SqliteStore,
    AppState,
    AppState,
) {
    let temporary = tempdir().expect("temporary state root");
    let config = StoreConfig::new(
        temporary.path().join("data/proqi.sqlite3"),
        temporary.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut store = SqliteStore::open(&config).expect("open");
    let source = AppState::new(
        SessionBoard::new(
            Session::new(
                ids.session_id(),
                temporary.path().to_path_buf(),
                Timestamp::from_millis(1),
            )
            .expect("source"),
            Vec::new(),
        )
        .expect("source board"),
    );
    let destination = AppState::new(
        SessionBoard::new(
            Session::new(
                ids.session_id(),
                temporary.path().to_path_buf(),
                Timestamp::from_millis(1),
            )
            .expect("destination"),
            Vec::new(),
        )
        .expect("destination board"),
    );
    for session in [&source.board.session, &destination.board.session] {
        store
            .commit(&OperationBatch::CreateSession(session.clone()))
            .expect("session");
    }
    (temporary, config, ids, store, source, destination)
}

fn make_request(
    store: &mut SqliteStore,
    source: &mut AppState,
    destination: &AppState,
    ids: &mut FakeIdGenerator,
) -> (SessionTransferBatchRequest, [ThoughtId; 2]) {
    let source_ids = [ids.thought_id(), ids.thought_id()];
    for (index, thought_id) in source_ids.iter().enumerate() {
        let operation = board_operation(
            source,
            Action::CreateThought {
                thought_id: *thought_id,
                operation_id: ids.operation_id(),
                content: format!("selected {index}"),
                annotations: Vec::new(),
                insertion_index: None,
                at: Timestamp::from_millis(2),
            },
        );
        commit(store, operation);
    }
    let items = source_ids
        .iter()
        .map(|id| {
            let thought = source.board.thought(*id).expect("source thought");
            TransferItem {
                source_thought_id: *id,
                destination_thought_id: ids.thought_id(),
                content: thought.content.clone(),
                annotations: thought.annotations.clone(),
                name: Some(ThoughtName::new("copy").expect("name")),
            }
        })
        .collect::<Vec<_>>();
    let request = SessionTransferBatchRequest {
        source_session_id: source.board.session.id,
        destination_session_id: destination.board.session.id,
        operation_id: ids.operation_id(),
        removal_operation_id: ids.operation_id(),
        items: items.clone(),
        remove_source: true,
    };
    (request, source_ids)
}

#[test]
fn whole_cohort_is_durable_before_one_atomic_source_removal() {
    let (_temporary, config, mut ids, mut store, mut source, mut destination) = setup();
    let (request, source_ids) = make_request(&mut store, &mut source, &destination, &mut ids);
    store
        .prepare_transfer(&request, Timestamp::from_millis(3))
        .expect("prepare");
    let destination_operation = board_operation(
        &mut destination,
        Action::CreateOwnedThoughts {
            operation_id: request.operation_id,
            items: request.items.clone(),
            at: Timestamp::from_millis(3),
        },
    );
    let destination_receipt = store
        .commit(&OperationBatch::Board {
            operation: destination_operation,
            semantic_fingerprint: None,
        })
        .expect("destination commit")
        .expect("destination receipt");
    assert_eq!(
        store
            .load_session(request.source_session_id)
            .expect("source before receipt")
            .board
            .live_thoughts()
            .len(),
        2
    );
    store
        .accept_transfer(&request, destination_receipt)
        .expect("accept cohort");
    let removal = board_operation(
        &mut source,
        Action::DeleteThoughts {
            operation_id: request.removal_operation_id,
            thought_ids: source_ids.to_vec(),
            kind: BoardOperationKind::TransferAndRemove,
            at: Timestamp::from_millis(4),
        },
    );
    let invalid = BoardOperation {
        id: ids.operation_id(),
        ..removal.clone()
    };
    assert!(
        store
            .finish_transfer(&request, Some(&invalid), "removed")
            .is_err()
    );
    assert_eq!(
        store
            .load_session(request.source_session_id)
            .expect("source after rejected removal")
            .board
            .live_thoughts()
            .len(),
        2
    );
    let source_receipt = store
        .finish_transfer(&request, Some(&removal), "removed")
        .expect("finish")
        .expect("source receipt");
    assert_eq!(source_receipt.session_id, request.source_session_id);
    let replay = store
        .finish_transfer(&request, Some(&removal), "removed")
        .expect("idempotent finish")
        .expect("source replay receipt");
    assert_eq!(replay.sequence, source_receipt.sequence);
    assert!(replay.idempotent_replay);
    verify_restart(store, &config, &request);
}

fn verify_restart(store: SqliteStore, config: &StoreConfig, request: &SessionTransferBatchRequest) {
    drop(store);
    let mut store = SqliteStore::open(config).expect("restart");
    assert!(
        store
            .pending_transfers(request.source_session_id)
            .expect("pending")
            .is_empty()
    );
    assert!(
        store
            .load_session(request.source_session_id)
            .expect("source")
            .board
            .live_thoughts()
            .is_empty()
    );
    let copied = store
        .load_session(request.destination_session_id)
        .expect("destination")
        .board
        .live_thoughts()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(copied.len(), 2);
    assert_eq!(
        copied
            .iter()
            .map(|thought| thought.content.as_str())
            .collect::<Vec<_>>(),
        vec!["selected 0", "selected 1"]
    );
    assert!(copied.iter().all(|thought| {
        thought
            .name
            .as_ref()
            .is_some_and(|name| name.as_str() == "copy")
    }));
}
