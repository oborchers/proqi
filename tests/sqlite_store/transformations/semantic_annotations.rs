use super::*;

fn shortcut(start: usize, end: usize) -> ContentAnnotation {
    serde_json::from_value(serde_json::json!({
        "start": start,
        "end": end,
        "kind": { "kind": "shortcut_emphasis" }
    }))
    .expect("valid application-owned shortcut annotation")
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one restart and compaction scenario keeps its durable fixture and oracle together"
)]
fn shortcut_ranges_survive_split_history_restart_fts_and_compaction() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_400_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-transform-shortcut"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("create session");
    let source = create_thought(&mut store, &mut state, &mut ids, "AA Enter ZZ", 2);
    drop(store);
    Connection::open(&fixture.config.database_path)
        .expect("annotation fixture")
        .execute(
            "UPDATE thoughts SET annotations_json = ?1 WHERE id = ?2",
            rusqlite::params![
                serde_json::to_string(&vec![shortcut(3, 8)]).expect("annotation JSON"),
                source.database_bytes().as_slice()
            ],
        )
        .expect("install application-owned shortcut fixture");

    let mut store = fixture.open();
    state = AppState::from_snapshot(store.load_session(session_id).expect("shortcut reopen"))
        .expect("rehydrate shortcut");
    let right = ids.thought_id();
    let split_id = ids.operation_id();
    persist_action(
        &mut store,
        &mut state,
        Action::SplitThought {
            thought_id: source,
            new_thought_id: right,
            operation_id: split_id,
            expected_content: "AA Enter ZZ".to_owned(),
            expected_annotations: vec![shortcut(3, 8)],
            at_byte: 3,
            at: Timestamp::from_millis(3),
        },
    );
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("split reopen");
    assert_eq!(
        snapshot.board.thought(right).expect("right").annotations,
        [shortcut(0, 5)]
    );
    assert!(search_matches(&mut store, session_id, "Enter"));
    state = AppState::from_snapshot(snapshot).expect("rehydrate split");
    persist_board_history(&mut store, &mut state, &mut ids, true, 4);
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("undo reopen");
    assert_eq!(
        snapshot.board.thought(source).expect("source").annotations,
        [shortcut(3, 8)]
    );
    state = AppState::from_snapshot(snapshot).expect("rehydrate undo");
    persist_board_history(&mut store, &mut state, &mut ids, false, 5);

    for index in 0..501_i64 {
        persist_action(
            &mut store,
            &mut state,
            Action::SetPresentation {
                operation_id: ids.operation_id(),
                thought_id: source,
                presentation: if index % 2 == 0 {
                    ThoughtPresentation::Collapsed
                } else {
                    ThoughtPresentation::Automatic
                },
                at: Timestamp::from_millis(10 + index),
            },
        );
    }
    store
        .compact_session(session_id)
        .expect("compact transform history");
    drop(store);

    let mut store = fixture.open();
    let snapshot = store.load_session(session_id).expect("compacted reopen");
    assert_eq!(
        snapshot.board.thought(right).expect("right").annotations,
        [shortcut(0, 5)]
    );
    assert!(search_matches(&mut store, session_id, "Enter"));
    assert!(matches!(
        store.operation_request(split_id).expect("split receipt"),
        Some(proqi::ports::store::StoredOperationRequest::Compacted {
            replay: proqi::ports::store::CompactedOperationRequest::Opaque,
            ..
        })
    ));
}
