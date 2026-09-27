//! Board and editor history compaction, replay, and cursor preservation contracts.

use proqi::{
    adapters::{
        memory::{FakeClock, FakeIdGenerator},
        runtime::FileRuntimeCoordinator,
    },
    application::{Action, AppState, SessionService, SessionServiceError},
    domain::{
        BoardMutation, BoardOperation, BoardOperationKind, OperationSequence, TextPosition,
        ThoughtPresentation, Timestamp, UndoScope,
    },
    ports::{
        environment::IdGenerator as _,
        store::{OperationBatch, Store as _},
    },
};
use sha2::{Digest as _, Sha256};

use super::{
    DatabaseFixture, create_thought, one_effect, persist_effect, session_state, test_path,
};

#[test]
fn board_compaction_preserves_current_state_undo_and_idempotency() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-compaction"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = ids.thought_id();
    let create_id = ids.operation_id();
    let create = one_effect(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: create_id,
            content: "retained exact content".to_owned(),
            annotations: Vec::new(),
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    );
    persist_effect(&mut store, &create);

    for index in 0..510_u64 {
        let collapsed = index % 2 == 0;
        let sequence = OperationSequence::new(index + 2);
        let operation = BoardOperation {
            id: ids.operation_id(),
            session_id,
            sequence,
            kind: BoardOperationKind::Collapse,
            forward: BoardMutation::SetPresentation {
                thought_id,
                presentation: presentation(collapsed),
            },
            inverse: BoardMutation::SetPresentation {
                thought_id,
                presentation: presentation(!collapsed),
            },
            created_at: Timestamp::from_millis(i64::try_from(index + 3).expect("timestamp")),
        };
        store
            .commit(&OperationBatch::Board {
                operation,
                semantic_fingerprint: None,
            })
            .expect("collapse commit");
    }

    store.compact_session(session_id).expect("compact session");
    let snapshot = store.load_session(session_id).expect("compacted snapshot");
    assert_eq!(snapshot.board_operations.len(), 500);
    assert_eq!(snapshot.board_history_cursor, 500);
    assert_eq!(
        snapshot.board.thought(thought_id).expect("thought").content,
        "retained exact content"
    );
    assert!(matches!(
        store
            .operation_request(create_id)
            .expect("operation lookup"),
        Some(proqi::ports::store::StoredOperationRequest::Compacted {
            replay: proqi::ports::store::CompactedOperationRequest::Add { .. },
            ..
        })
    ));

    let mut restored = AppState::from_snapshot(snapshot).expect("restored state");
    let undo = one_effect(
        &mut restored,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(600),
        },
    );
    persist_effect(&mut store, &undo);
    let after_undo = store.load_session(session_id).expect("undo snapshot");
    assert_eq!(after_undo.board_history_cursor, 499);
}

#[test]
fn semantic_fingerprint_distinguishes_identical_extract_results_after_compaction() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_726_100_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-semantic-compaction"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "aaa", 2);
    let extract_id = ids.operation_id();
    let collapse_ids = (0..501_u64).map(|_| ids.operation_id()).collect::<Vec<_>>();
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let coordinator = FileRuntimeCoordinator::new(
        fixture.temporary.path().join("runtime"),
        ids.instance_id(),
        test_path("proqi-semantic-compaction-owner"),
        Timestamp::from_millis(3),
        "semantic-compaction-test",
    )
    .expect("runtime coordinator");
    let cwd = test_path("proqi-semantic-compaction-cli");
    let expected: [u8; 32] = Sha256::digest(b"aaa").into();

    let mut service = SessionService::new(&mut store, &coordinator, &clock, &mut ids, cwd.clone())
        .expect("session service");
    let original = service
        .extract_thought(session_id, thought_id, 0..1, expected, Some(extract_id))
        .expect("extract request");
    for (index, operation_id) in collapse_ids.into_iter().enumerate() {
        service
            .set_thought_collapsed(session_id, thought_id, index % 2 == 0, operation_id)
            .expect("compaction pressure");
    }
    drop(service);
    assert!(matches!(
        store
            .operation_request(extract_id)
            .expect("extract receipt"),
        Some(proqi::ports::store::StoredOperationRequest::Compacted {
            semantic_fingerprint: Some(_),
            ..
        })
    ));
    drop(store);

    let mut store = fixture.open();
    let mut service = SessionService::new(&mut store, &coordinator, &clock, &mut ids, cwd)
        .expect("reopened service");
    let replay = service
        .extract_thought(session_id, thought_id, 0..1, expected, Some(extract_id))
        .expect("exact compacted replay");
    assert!(replay.receipt.idempotent_replay);
    assert_eq!(replay.item_ids, original.item_ids);
    assert!(matches!(
        service.extract_thought(session_id, thought_id, 1..2, expected, Some(extract_id)),
        Err(SessionServiceError::IdempotencyConflict)
    ));
}

#[test]
fn semantic_fingerprint_preserves_exact_revision_replay_after_compaction() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_726_200_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-revision-semantic-compaction"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "before", 2);
    let target_revision = ids.revision_id();
    let pressure_ids = (0..205_u64).map(|_| ids.revision_id()).collect::<Vec<_>>();
    let coordinator = FileRuntimeCoordinator::new(
        fixture.temporary.path().join("revision-runtime"),
        ids.instance_id(),
        test_path("proqi-revision-semantic-owner"),
        Timestamp::from_millis(3),
        "revision-semantic-test",
    )
    .expect("runtime coordinator");
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let cwd = test_path("proqi-revision-semantic-cli");
    let before_digest: [u8; 32] = Sha256::digest(b"before").into();
    let mut service = SessionService::new(&mut store, &coordinator, &clock, &mut ids, cwd.clone())
        .expect("session service");
    let original = service
        .replace_thought(
            session_id,
            thought_id,
            "first".to_owned(),
            Some(before_digest),
            target_revision,
        )
        .expect("first replacement");
    let mut current = "first".to_owned();
    for (index, revision_id) in pressure_ids.into_iter().enumerate() {
        let expected: [u8; 32] = Sha256::digest(current.as_bytes()).into();
        let next = format!("revision-{index}");
        service
            .replace_thought(
                session_id,
                thought_id,
                next.clone(),
                Some(expected),
                revision_id,
            )
            .expect("revision compaction pressure");
        current = next;
    }
    drop(service);
    assert_compacted_revision(&mut store, target_revision);
    drop(store);

    let mut store = fixture.open();
    let mut service = SessionService::new(&mut store, &coordinator, &clock, &mut ids, cwd)
        .expect("reopened service");
    let replay = service
        .replace_thought(
            session_id,
            thought_id,
            "first".to_owned(),
            Some(before_digest),
            target_revision,
        )
        .expect("exact revision replay");
    assert!(replay.receipt.idempotent_replay);
    assert_eq!(replay.receipt.session_id, original.receipt.session_id);
    assert_eq!(replay.receipt.sequence, original.receipt.sequence);
    assert_eq!(replay.receipt.identity, original.receipt.identity);
    assert!(matches!(
        service.replace_thought(
            session_id,
            thought_id,
            "changed".to_owned(),
            Some(before_digest),
            target_revision,
        ),
        Err(SessionServiceError::Store(
            proqi::ports::store::StoreError::Conflict(_)
        ))
    ));
}

fn assert_compacted_revision(
    store: &mut impl proqi::ports::store::Store,
    revision_id: proqi::domain::RevisionId,
) {
    assert!(matches!(
        store
            .revision_request(revision_id)
            .expect("revision receipt"),
        Some(proqi::ports::store::StoredOperationRequest::Compacted {
            semantic_fingerprint: Some(_),
            ..
        })
    ));
}

#[test]
fn compaction_preserves_every_redo_entry() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_100_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-redo-compaction"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "thought", 2);
    for index in 0..504_u64 {
        let collapsed = index % 2 == 0;
        store
            .commit(&OperationBatch::Board {
                operation: BoardOperation {
                    id: ids.operation_id(),
                    session_id,
                    sequence: OperationSequence::new(index + 2),
                    kind: BoardOperationKind::Collapse,
                    forward: BoardMutation::SetPresentation {
                        thought_id,
                        presentation: presentation(collapsed),
                    },
                    inverse: BoardMutation::SetPresentation {
                        thought_id,
                        presentation: presentation(!collapsed),
                    },
                    created_at: Timestamp::from_millis(
                        i64::try_from(index + 3).expect("timestamp"),
                    ),
                },
                semantic_fingerprint: None,
            })
            .expect("collapse commit");
    }
    for index in 0..10_u64 {
        store
            .commit(&OperationBatch::HistoryMove {
                operation_id: ids.operation_id(),
                session_id,
                scope: UndoScope::Board,
                undo: true,
                sequence: OperationSequence::new(506 + index),
                at: Timestamp::from_millis(i64::try_from(600 + index).expect("timestamp")),
                semantic_fingerprint: None,
            })
            .expect("undo commit");
    }

    store
        .compact_session(session_id)
        .expect("compact redo history");
    let snapshot = store.load_session(session_id).expect("compacted snapshot");
    assert_eq!(
        snapshot.board_operations.len() - snapshot.board_history_cursor,
        10
    );
    assert!(snapshot.board_history_cursor >= 1);
}

const fn presentation(collapsed: bool) -> ThoughtPresentation {
    if collapsed {
        ThoughtPresentation::Collapsed
    } else {
        ThoughtPresentation::Automatic
    }
}

#[test]
fn editor_compaction_keeps_recent_revisions_and_cursor() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-editor-compaction"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let thought_id = create_thought(&mut store, &mut state, &mut ids, "0", 2);
    let mut content = "0".to_owned();
    for index in 1..=205_u64 {
        let next = index.to_string();
        let edit = one_effect(
            &mut state,
            Action::EditThought {
                thought_id,
                revision_id: ids.revision_id(),
                before_content: content.clone(),
                after_content: next.clone(),
                before_annotations: Vec::new(),
                after_annotations: Vec::new(),
                before_cursor: TextPosition::new(0, content.len()),
                after_cursor: TextPosition::new(0, next.len()),
                at: Timestamp::from_millis(i64::try_from(index + 2).expect("timestamp")),
            },
        );
        persist_effect(&mut store, &edit);
        content = next;
    }

    store.compact_session(session_id).expect("compact editor");
    let snapshot = store.load_session(session_id).expect("snapshot");
    assert_eq!(snapshot.revisions.len(), 200);
    assert_eq!(snapshot.editor_history_cursors, [(thought_id, 200)]);
    assert_eq!(
        snapshot.board.thought(thought_id).expect("thought").content,
        "205"
    );

    let mut restored = AppState::from_snapshot(snapshot).expect("restore state");
    let undo = one_effect(
        &mut restored,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(300),
        },
    );
    persist_effect(&mut store, &undo);
    assert_eq!(
        store
            .load_session(session_id)
            .expect("undone snapshot")
            .board
            .thought(thought_id)
            .expect("thought")
            .content,
        "204"
    );
}
