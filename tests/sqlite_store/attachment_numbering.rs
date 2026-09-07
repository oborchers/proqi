//! Durable session allocation, deletion, replay and one-time legacy synthesis.

use super::{DatabaseFixture, persist_effect, session_state, test_path};
use proqi::{
    adapters::memory::FakeIdGenerator,
    application::{Action, AppState, reduce},
    domain::{
        ContentAnnotation, ContentAnnotationKind, TextPosition, ThoughtId, Timestamp, UndoScope,
    },
    ports::{
        environment::IdGenerator,
        store::{OperationBatch, Store},
    },
};

fn annotation(start: usize, end: usize, image: bool) -> ContentAnnotation {
    ContentAnnotation {
        start,
        end,
        kind: ContentAnnotationKind::Attachment {
            ordinal: None,
            image,
            display_name: "asset".to_owned(),
        },
    }
}

fn ordinal(state: &AppState, thought: ThoughtId) -> u64 {
    let ContentAnnotationKind::Attachment { ordinal, .. } =
        state.board.thought(thought).expect("thought").annotations[0].kind
    else {
        panic!("attachment")
    };
    ordinal.expect("assigned").get()
}

fn apply(store: &mut proqi::adapters::sqlite::SqliteStore, state: &mut AppState, action: Action) {
    for effect in reduce(state, action).expect("reduce") {
        if effect.persistence_batch().is_some() {
            persist_effect(store, &effect);
        }
    }
}

fn create(
    store: &mut proqi::adapters::sqlite::SqliteStore,
    state: &mut AppState,
    ids: &mut FakeIdGenerator,
    image: bool,
) -> ThoughtId {
    let id = ids.thought_id();
    let content = "/offline/Grüße asset.png".to_owned();
    let annotations = vec![annotation(0, content.len(), image)];
    apply(
        store,
        state,
        Action::CreateThought {
            thought_id: id,
            operation_id: ids.operation_id(),
            content,
            annotations,
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    id
}

#[test]
fn long_mixed_sequence_survives_shuffle_deletion_history_and_restart_without_reuse() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("numbering"));
    let session = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let mut images = Vec::new();
    for index in 1..=14 {
        let image = create(&mut store, &mut state, &mut ids, true);
        assert_eq!(ordinal(&state, image), index);
        images.push(image);
        if index % 3 == 0 {
            let file = create(&mut store, &mut state, &mut ids, false);
            assert_eq!(ordinal(&state, file), index / 3);
        }
    }
    for image in &images {
        apply(
            &mut store,
            &mut state,
            Action::MoveThought {
                thought_id: *image,
                operation_id: ids.operation_id(),
                to: 0,
                at: Timestamp::from_millis(3),
            },
        );
    }
    for (index, image) in images.iter().enumerate() {
        assert_eq!(ordinal(&state, *image), index as u64 + 1);
    }
    let removed = images[13];
    apply(
        &mut store,
        &mut state,
        Action::DeleteThought {
            thought_id: removed,
            operation_id: ids.operation_id(),
            kind: proqi::domain::BoardOperationKind::Delete,
            at: Timestamp::from_millis(4),
        },
    );
    drop(store);
    let mut store = fixture.open();
    let mut state =
        AppState::from_snapshot(store.load_session(session).expect("snapshot")).expect("state");
    apply(
        &mut store,
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(5),
        },
    );
    assert_eq!(ordinal(&state, removed), 14);
    apply(
        &mut store,
        &mut state,
        Action::Redo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(6),
        },
    );
    let next = create(&mut store, &mut state, &mut ids, true);
    assert_eq!(ordinal(&state, next), 15);
    assert_eq!(
        store
            .load_session(session)
            .expect("durable")
            .board
            .attachment_counters()
            .image(),
        15
    );
}

#[test]
fn ambiguous_legacy_coalesced_edit_is_synthesized_once_and_replays_stably() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("legacy-numbering"));
    let session = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let (id, content) = coalesced_legacy_edit(&mut store, &mut state, &mut ids);
    drop(store);
    downgrade_to_legacy(&fixture);
    let mut store = fixture.open();
    let migrated = store.load_session(session).expect("migrated");
    assert_eq!(migrated.board.attachment_counters().image(), 2);
    let mut state = AppState::from_snapshot(migrated).expect("state");
    assert_eq!(ordinal(&state, id), 1);
    for _ in 0..2 {
        apply(
            &mut store,
            &mut state,
            Action::Undo {
                operation_id: ids.operation_id(),
                scope: UndoScope::Editor { thought_id: id },
                at: Timestamp::from_millis(4),
            },
        );
        let thought = state.board.thought(id).expect("thought");
        assert_eq!(thought.content, content);
        assert_eq!(
            thought
                .annotations
                .iter()
                .map(|a| match a.kind {
                    ContentAnnotationKind::Attachment { ordinal, .. } =>
                        ordinal.expect("ordinal").get(),
                    _ => 0,
                })
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        apply(
            &mut store,
            &mut state,
            Action::Redo {
                operation_id: ids.operation_id(),
                scope: UndoScope::Editor { thought_id: id },
                at: Timestamp::from_millis(5),
            },
        );
        assert_eq!(ordinal(&state, id), 1);
    }
    drop(store);
    let mut store = fixture.open();
    let restored = store.load_session(session).expect("reopen");
    assert_eq!(restored.board.thoughts(), state.board.thoughts());
    assert_eq!(
        restored.board.attachment_counters(),
        state.board.attachment_counters()
    );
    let mut state = AppState::from_snapshot(restored).expect("state");
    let next = create(&mut store, &mut state, &mut ids, true);
    assert_eq!(ordinal(&state, next), 3);
}

fn coalesced_legacy_edit(
    store: &mut proqi::adapters::sqlite::SqliteStore,
    state: &mut AppState,
    ids: &mut FakeIdGenerator,
) -> (ThoughtId, String) {
    let id = ids.thought_id();
    let path = "/offline/same.png";
    let content = format!("{path}\n{path}");
    apply(
        store,
        state,
        Action::CreateThought {
            thought_id: id,
            operation_id: ids.operation_id(),
            content: content.clone(),
            annotations: vec![
                annotation(0, path.len(), true),
                annotation(path.len() + 1, content.len(), true),
            ],
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    let before_annotations = state
        .board
        .thought(id)
        .expect("thought")
        .annotations
        .clone();
    let mut after = before_annotations[1].clone();
    after.start = 0;
    after.end = path.len();
    apply(
        store,
        state,
        Action::EditThought {
            thought_id: id,
            revision_id: ids.revision_id(),
            before_content: content.clone(),
            after_content: path.to_owned(),
            before_annotations,
            after_annotations: vec![after],
            before_cursor: TextPosition::new(0, 0),
            after_cursor: TextPosition::new(0, 0),
            at: Timestamp::from_millis(3),
        },
    );
    (id, content)
}

pub(super) fn downgrade_to_legacy(fixture: &DatabaseFixture) {
    let connection = rusqlite::Connection::open(&fixture.config.database_path).expect("database");
    for (table, column, key) in [
        ("thoughts", "annotations_json", "id"),
        ("board_operations", "payload_json", "id"),
        ("thought_revisions", "payload_json", "id"),
        ("commit_receipts", "request_json", "external_id"),
    ] {
        let mut statement = connection
            .prepare(&format!("SELECT {key}, {column} FROM {table}"))
            .expect("query");
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
            })
            .expect("rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        for (id, encoded) in rows {
            let mut value: serde_json::Value = serde_json::from_str(&encoded).expect("json");
            strip_ordinals(&mut value);
            connection
                .execute(
                    &format!("UPDATE {table} SET {column} = ?2 WHERE {key} = ?1"),
                    rusqlite::params![id, value.to_string()],
                )
                .expect("legacy payload");
        }
    }
    connection.execute_batch("ALTER TABLE sessions DROP COLUMN attachment_image_high; ALTER TABLE sessions DROP COLUMN attachment_file_high; DELETE FROM migration_history WHERE version = 14; UPDATE schema_meta SET schema_version = 13, storage_protocol = 12;").expect("legacy schema");
}

fn strip_ordinals(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            object.remove("ordinal");
            for child in object.values_mut() {
                strip_ordinals(child);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                strip_ordinals(child);
            }
        }
        _ => {}
    }
}

#[path = "attachment_numbering/capture.rs"]
mod capture;

#[path = "attachment_numbering/integrity.rs"]
mod integrity;

#[path = "attachment_numbering/structural.rs"]
mod structural;

#[path = "attachment_numbering/repeated_snapshot.rs"]
mod repeated_snapshot;
