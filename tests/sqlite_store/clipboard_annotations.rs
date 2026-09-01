use super::*;

fn durable_effect(effects: &[Effect]) -> &Effect {
    effects
        .iter()
        .find(|effect| effect.persistence_batch().is_some())
        .expect("durable effect")
}

fn annotated_content() -> (String, ContentAnnotation) {
    let content = "Before /missing/Grüße.png after".to_owned();
    let start = content.find('/').expect("path start");
    let end = start + "/missing/Grüße.png".len();
    let annotation = ContentAnnotation {
        start,
        end,
        kind: ContentAnnotationKind::Attachment {
            image: true,
            display_name: "Grüße.png".to_owned(),
        },
    };
    (content, annotation)
}

#[test]
fn annotated_cut_and_one_board_undo_survive_separate_restarts() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_350_000);
    let mut state = session_state(&mut ids, &test_path("proqi-clipboard-cut-undo"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = ids.thought_id();
    let (content, annotation) = annotated_content();
    let create = reduce(
        &mut state,
        Action::CreateThought {
            thought_id,
            operation_id: ids.operation_id(),
            content: content.clone(),
            annotations: vec![annotation.clone()],
            insertion_index: None,
            at: Timestamp::from_millis(2),
        },
    )
    .expect("create");
    persist_effect(&mut store, durable_effect(&create));
    let request_id = ids.request_id();
    reduce(
        &mut state,
        Action::CutThoughts {
            request_id,
            operation_id: ids.operation_id(),
            thought_ids: vec![thought_id],
            at: Timestamp::from_millis(3),
        },
    )
    .expect("cut request");
    let cut = reduce(
        &mut state,
        Action::ClipboardResult {
            request_id,
            result: Ok(()),
        },
    )
    .expect("cut result");
    persist_effect(&mut store, durable_effect(&cut));
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("cut reopen");
    assert!(
        !snapshot
            .board
            .thought(thought_id)
            .expect("tombstone")
            .is_live()
    );
    state = AppState::from_snapshot(snapshot).expect("rehydrate cut");
    let undo = reduce(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Board,
            at: Timestamp::from_millis(4),
        },
    )
    .expect("undo");
    persist_effect(&mut store, durable_effect(&undo));
    drop(store);

    let restored = fixture
        .open()
        .load_session(session_id)
        .expect("undo reopen");
    let thought = restored
        .board
        .thought(thought_id)
        .expect("restored thought");
    assert!(thought.is_live());
    assert_eq!(thought.content, content);
    assert_eq!(thought.annotations, [annotation]);
    assert_eq!(restored.board_history_cursor, 1);
}
