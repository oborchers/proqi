use std::fs;

use tempfile::tempdir;

use crate::{
    adapters::{
        memory::FakeIdGenerator,
        process::CancellationFlag,
        runtime::FileRuntimeCoordinator,
        sqlite::{SqliteStore, StoreConfig},
    },
    application::{Action, AppState, Effect, reduce},
    domain::{Session, Timestamp},
    ports::{
        environment::IdGenerator,
        runtime::RuntimeCoordinator,
        store::{MigrationMode, OperationBatch, Store},
        transfer::{SessionTransferBatchRequest, TransferItem},
    },
};

use super::{TransferRuntime, deliver_batch};

#[test]
fn inactive_destination_selected_transfer_keeps_the_live_source_owner_intact() {
    assert_inactive_destination_transfer(2);
}

#[test]
fn inactive_destination_focused_transfer_keeps_the_live_source_owner_intact() {
    assert_inactive_destination_transfer(1);
}

fn assert_inactive_destination_transfer(item_count: usize) {
    let temporary = tempdir().expect("isolated transfer state");
    let config = StoreConfig::new(
        temporary.path().join("data/proqi.sqlite3"),
        temporary.path().join("backups"),
        MigrationMode::Allow,
        Timestamp::from_millis(1),
    );
    let mut store = SqliteStore::open(&config).expect("store");
    let mut ids = FakeIdGenerator::new(1_725_300_000_000);
    let source_id = ids.session_id();
    let destination_id = ids.session_id();
    let source_thought_ids = [ids.thought_id(), ids.thought_id()];
    let destination_thought_ids = [ids.thought_id(), ids.thought_id()];
    create_sessions(&mut store, [source_id, destination_id], temporary.path());
    create_source_thoughts(&mut store, &mut ids, source_id, source_thought_ids);

    let coordinator = FileRuntimeCoordinator::new(
        temporary.path().join("runtime"),
        ids.instance_id(),
        temporary.path().to_path_buf(),
        Timestamp::from_millis(3),
        "test",
    )
    .expect("coordinator");
    let source_lease = coordinator
        .acquire_session(source_id)
        .expect("source owner");
    let source_metadata = temporary
        .path()
        .join("runtime/instances")
        .join(format!("{}.json", source_lease.info().instance_id));
    let original_metadata = fs::read(&source_metadata).expect("source metadata");
    let request = selected_request(
        &mut ids,
        source_id,
        destination_id,
        source_thought_ids,
        destination_thought_ids,
        item_count,
    );
    let mut runtime = TransferRuntime::new(
        coordinator.clone(),
        temporary.path().to_path_buf(),
        CancellationFlag::default(),
    );
    assert_unavailable_destination_keeps_sources(
        &mut store,
        &mut runtime,
        &coordinator,
        ids.instance_id(),
        &request,
    );
    let receipt = deliver_batch(&mut store, &mut runtime, &request)
        .expect("inactive destination must accept the complete cohort");
    assert_eq!(receipt.session_id, destination_id);
    assert_eq!(
        fs::read(&source_metadata).expect("source metadata"),
        original_metadata
    );
    assert_eq!(
        coordinator.active_instances().expect("active owners").len(),
        1
    );
    assert_destination_cohort(
        &mut store,
        source_id,
        destination_id,
        destination_thought_ids,
        item_count,
    );
    assert!(source_metadata.exists());

    assert_restart_replay(
        store,
        &config,
        &mut runtime,
        &request,
        &receipt,
        &source_metadata,
        &original_metadata,
    );
}

fn assert_unavailable_destination_keeps_sources(
    store: &mut SqliteStore,
    runtime: &mut TransferRuntime,
    coordinator: &FileRuntimeCoordinator,
    destination_instance_id: crate::domain::InstanceId,
    request: &SessionTransferBatchRequest,
) {
    let owner =
        coordinator.transient_session_owner(destination_instance_id, Timestamp::from_millis(4));
    let destination_lease = owner
        .acquire_session(request.destination_session_id)
        .expect("held destination owner");
    let error = deliver_batch(store, runtime, request).expect_err("unavailable owner must fail");
    assert!(error.contains("does not advertise control forwarding"));
    assert_eq!(
        store
            .load_session(request.source_session_id)
            .expect("source after failed send")
            .board
            .live_thoughts()
            .len(),
        2
    );
    assert!(
        store
            .load_session(request.destination_session_id)
            .expect("destination after failed send")
            .board
            .live_thoughts()
            .is_empty()
    );
    drop(destination_lease);
}

fn create_sessions(
    store: &mut SqliteStore,
    session_ids: [crate::domain::SessionId; 2],
    path: &std::path::Path,
) {
    for session_id in session_ids {
        let session = Session::new(session_id, path.to_path_buf(), Timestamp::from_millis(1))
            .expect("session");
        store
            .commit(&OperationBatch::CreateSession(session))
            .expect("create session");
    }
}

fn create_source_thoughts(
    store: &mut SqliteStore,
    ids: &mut FakeIdGenerator,
    source_id: crate::domain::SessionId,
    source_thought_ids: [crate::domain::ThoughtId; 2],
) {
    let source_board = store.load_session(source_id).expect("source").board;
    let mut source_state = AppState::new(source_board);
    for (index, thought_id) in source_thought_ids.into_iter().enumerate() {
        let effects = reduce(
            &mut source_state,
            Action::CreateThought {
                thought_id,
                operation_id: ids.operation_id(),
                content: format!("selected source {index}"),
                annotations: Vec::new(),
                insertion_index: None,
                at: Timestamp::from_millis(2),
            },
        )
        .expect("create thought");
        let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
            panic!("one source Board operation");
        };
        store
            .commit(&OperationBatch::Board {
                operation: operation.clone(),
                semantic_fingerprint: None,
            })
            .expect("source commit");
    }
}

fn assert_destination_cohort(
    store: &mut SqliteStore,
    source_id: crate::domain::SessionId,
    destination_id: crate::domain::SessionId,
    destination_thought_ids: [crate::domain::ThoughtId; 2],
    item_count: usize,
) {
    let destination = store.load_session(destination_id).expect("destination");
    assert_eq!(
        destination
            .board
            .live_thoughts()
            .iter()
            .map(|thought| (thought.id, thought.content.as_str()))
            .collect::<Vec<_>>(),
        [
            (destination_thought_ids[0], "selected source 0"),
            (destination_thought_ids[1], "selected source 1"),
        ]
        .into_iter()
        .take(item_count)
        .collect::<Vec<_>>()
    );
    assert_eq!(
        store
            .load_session(source_id)
            .expect("source")
            .board
            .live_thoughts()
            .len(),
        2
    );
}

fn assert_restart_replay(
    store: SqliteStore,
    config: &StoreConfig,
    runtime: &mut TransferRuntime,
    request: &SessionTransferBatchRequest,
    receipt: &crate::ports::store::CommitReceipt,
    source_metadata: &std::path::Path,
    original_metadata: &[u8],
) {
    drop(store);
    let mut reopened = SqliteStore::open(config).expect("restart store");
    let replay = deliver_batch(&mut reopened, runtime, request)
        .expect("retry must resolve the accepted cohort");
    assert_eq!(replay.identity, receipt.identity);
    assert_eq!(replay.sequence, receipt.sequence);
    assert_eq!(
        reopened
            .load_session(request.destination_session_id)
            .expect("destination after retry")
            .board
            .live_thoughts()
            .len(),
        request.items.len()
    );
    assert_eq!(
        fs::read(source_metadata).expect("source metadata after retry"),
        original_metadata
    );
}

fn selected_request(
    ids: &mut FakeIdGenerator,
    source_id: crate::domain::SessionId,
    destination_id: crate::domain::SessionId,
    source_thought_ids: [crate::domain::ThoughtId; 2],
    destination_thought_ids: [crate::domain::ThoughtId; 2],
    item_count: usize,
) -> SessionTransferBatchRequest {
    SessionTransferBatchRequest {
        source_session_id: source_id,
        destination_session_id: destination_id,
        operation_id: ids.operation_id(),
        removal_operation_id: ids.operation_id(),
        items: source_thought_ids
            .into_iter()
            .zip(destination_thought_ids)
            .take(item_count)
            .enumerate()
            .map(
                |(index, (source_thought_id, destination_thought_id))| TransferItem {
                    source_thought_id,
                    destination_thought_id,
                    content: format!("selected source {index}"),
                    annotations: Vec::new(),
                    name: None,
                },
            )
            .collect(),
        remove_source: true,
    }
}
