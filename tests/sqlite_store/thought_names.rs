//! Durable optional-name contracts, including mixed separator history.

use proqi::{
    adapters::{memory::FakeIdGenerator, sqlite::SqliteStore},
    application::{Action, AppState},
    domain::{BoardOperationKind, SeparatorId, ThoughtName, Timestamp, UndoScope},
    ports::{
        environment::IdGenerator,
        store::{OperationBatch, Store},
    },
};

use super::{
    DatabaseFixture, create_thought, one_effect, persist_effect, session_state, test_path,
};

#[test]
fn names_survive_restart_and_board_undo_redo_without_changing_body() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-thought-name"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "exact body", 2);
    let name = proqi::domain::ThoughtName::new("Unicode 設計").expect("name");
    let effect = one_effect(
        &mut state,
        Action::RenameThought {
            operation_id: ids.operation_id(),
            thought_id,
            name: Some(name.clone()),
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &effect);

    let reopened = fixture
        .open()
        .load_session(session_id)
        .expect("restart load");
    let thought = reopened.board.thought(thought_id).expect("thought");
    assert_eq!(thought.content, "exact body");
    assert_eq!(thought.name.as_ref(), Some(&name));

    for (undo, expected) in [(true, None), (false, Some(name.clone()))] {
        let effect = one_effect(
            &mut state,
            if undo {
                Action::Undo {
                    operation_id: ids.operation_id(),
                    scope: UndoScope::Board,
                    at: Timestamp::from_millis(4),
                }
            } else {
                Action::Redo {
                    operation_id: ids.operation_id(),
                    scope: UndoScope::Board,
                    at: Timestamp::from_millis(5),
                }
            },
        );
        persist_effect(&mut store, &effect);
        let loaded = store.load_session(session_id).expect("history load");
        let thought = loaded.board.thought(thought_id).expect("thought");
        assert_eq!(thought.content, "exact body");
        assert_eq!(thought.name, expected);
    }
}

#[test]
fn same_value_rename_receipt_is_retry_safe_without_a_history_unit() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_100_000);
    let mut state = session_state(&mut ids, &test_path("proqi-thought-name-noop"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "body", 2);
    let effect = one_effect(
        &mut state,
        Action::RenameThought {
            operation_id: ids.operation_id(),
            thought_id,
            name: None,
            at: Timestamp::from_millis(3),
        },
    );
    let batch = effect.persistence_batch().expect("durable batch");
    let first = store
        .commit(&batch)
        .expect("first commit")
        .expect("receipt");
    let second = store.commit(&batch).expect("retry").expect("receipt");
    assert!(!first.idempotent_replay);
    assert!(second.idempotent_replay);
    let cursor: u32 = rusqlite::Connection::open(&fixture.config.database_path)
        .expect("database")
        .query_row(
            "SELECT board_history_cursor FROM sessions WHERE id = ?1",
            [session_id.database_bytes().as_slice()],
            |row| row.get(0),
        )
        .expect("cursor");
    assert_eq!(cursor, 1);
}

#[test]
fn name_and_separator_history_share_order_without_sharing_payload() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_726_200_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-name-separator-history"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let first = create_thought(&mut store, &mut state, &mut ids, "first body", 2);
    let separator = ids.separator_id();
    persist_separator_insert(&mut store, &mut state, &mut ids, separator);
    let second = create_thought(&mut store, &mut state, &mut ids, "second body", 4);
    let name = ThoughtName::new("Named first").expect("name");
    let rename = one_effect(
        &mut state,
        Action::RenameThought {
            operation_id: ids.operation_id(),
            thought_id: first,
            name: Some(name.clone()),
            at: Timestamp::from_millis(5),
        },
    );
    persist_effect(&mut store, &rename);
    persist_separator_reorder_history(&mut store, &mut state, &mut ids, separator);
    drop(store);

    let snapshot = fixture
        .open()
        .load_session(session_id)
        .expect("integrated restart");
    assert_eq!(
        snapshot
            .board
            .live_items()
            .into_iter()
            .map(proqi::domain::BoardItemRef::id)
            .collect::<Vec<_>>(),
        vec![separator.into(), first.into(), second.into()]
    );
    let first_thought = snapshot.board.thought(first).expect("first thought");
    assert_eq!(first_thought.content, "first body");
    assert_eq!(first_thought.name.as_ref(), Some(&name));
    assert_eq!(
        snapshot
            .board
            .thought(second)
            .expect("second thought")
            .content,
        "second body"
    );
}

fn persist_separator_insert(
    store: &mut SqliteStore,
    state: &mut AppState,
    ids: &mut FakeIdGenerator,
    separator: SeparatorId,
) {
    let insert = one_effect(
        state,
        Action::InsertSeparator {
            separator_id: separator,
            operation_id: ids.operation_id(),
            insertion_index: 1,
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(store, &insert);
}

fn persist_separator_reorder_history(
    store: &mut SqliteStore,
    state: &mut AppState,
    ids: &mut FakeIdGenerator,
    separator: SeparatorId,
) {
    let moved = one_effect(
        state,
        Action::MoveItem {
            operation_id: ids.operation_id(),
            item_id: separator.into(),
            to: 0,
            at: Timestamp::from_millis(6),
        },
    );
    persist_effect(store, &moved);
    let deleted = one_effect(
        state,
        Action::DeleteItems {
            operation_id: ids.operation_id(),
            item_ids: vec![separator.into()],
            kind: BoardOperationKind::Delete,
            at: Timestamp::from_millis(7),
        },
    );
    persist_effect(store, &deleted);

    for (undo, at) in [(true, 8), (true, 9), (false, 10)] {
        let history = one_effect(
            state,
            if undo {
                Action::Undo {
                    operation_id: ids.operation_id(),
                    scope: UndoScope::Board,
                    at: Timestamp::from_millis(at),
                }
            } else {
                Action::Redo {
                    operation_id: ids.operation_id(),
                    scope: UndoScope::Board,
                    at: Timestamp::from_millis(at),
                }
            },
        );
        persist_effect(store, &history);
    }
}
