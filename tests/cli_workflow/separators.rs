use proqi::{
    adapters::{
        runtime::SystemIdGenerator,
        sqlite::{SqliteStore, StoreConfig},
    },
    application::{Action, AppState, Effect, reduce},
    domain::Timestamp,
    ports::{
        environment::IdGenerator,
        store::{MigrationMode, OperationBatch, Store},
    },
};

use super::{add_with_idempotency, create_session, operation_id, success};

#[test]
fn thought_listing_reports_structural_order_without_masquerading_separator_content() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let root = temporary.path();
    let session = create_session(root);
    let first = add_with_idempotency(root, &session, "first");
    let second = add_with_idempotency(root, &session, "second");
    let config = StoreConfig::new(
        root.join("data/proqi.sqlite3"),
        root.join("data/backups"),
        MigrationMode::Refuse,
        Timestamp::from_millis(1_725_999_000_000),
    );
    let mut store = SqliteStore::open(&config).expect("open CLI store");
    let session_id = session.parse().expect("session ID");
    let snapshot = store.load_session(session_id).expect("snapshot");
    let mut state = AppState::from_snapshot(snapshot).expect("state");
    let mut ids = SystemIdGenerator;
    let separator_id = ids.separator_id();
    let effects = reduce(
        &mut state,
        Action::InsertSeparator {
            separator_id,
            operation_id: ids.operation_id(),
            insertion_index: 1,
            at: Timestamp::from_millis(1_725_999_000_001),
        },
    )
    .expect("insert separator");
    let [Effect::CommitBoardOperation(operation)] = effects.as_slice() else {
        panic!("separator commit");
    };
    store
        .commit(&OperationBatch::Board(operation.clone()))
        .expect("commit separator");
    drop(store);

    let listed = success(root, &["thoughts", "list", &session], None);
    assert_eq!(listed["thoughts"].as_array().expect("thoughts").len(), 2);
    let items = listed["items"].as_array().expect("items");
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["kind"], "thought");
    assert_eq!(items[0]["id"], first);
    assert_eq!(items[1]["kind"], "separator");
    assert_eq!(items[1]["id"], separator_id.to_string());
    assert!(items[1].get("content").is_none());
    assert!(items[1].get("annotations").is_none());
    assert_eq!(items[2]["kind"], "thought");
    assert_eq!(items[2]["id"], second);

    let move_operation = operation_id();
    success(
        root,
        &[
            "thoughts",
            "move",
            &session,
            &second,
            "0",
            "--operation-id",
            &move_operation,
        ],
        None,
    );
    let reordered = success(root, &["thoughts", "list", &session], None);
    assert_eq!(reordered["items"][0]["id"], second);
    assert_eq!(reordered["items"][1]["id"], first);
    assert_eq!(reordered["items"][2]["id"], separator_id.to_string());
}
