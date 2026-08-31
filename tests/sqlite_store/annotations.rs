use super::*;

#[test]
fn folded_provenance_and_editor_undo_survive_reopen() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_000_000);
    let mut state = session_state(&mut ids, &test_path("proqi-annotations"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = ids.thought_id();
    let content = "/private/tmp/screenshot.png".to_owned();
    let annotation = ContentAnnotation {
        start: 0,
        end: content.len(),
        kind: ContentAnnotationKind::Attachment {
            image: true,
            display_name: "screenshot.png".to_owned(),
        },
    };
    let create_effects = reduce(
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
    assert!(
        create_effects
            .iter()
            .any(|effect| matches!(effect, Effect::CheckAttachments(_)))
    );
    let create = create_effects
        .iter()
        .find(|effect| effect.persistence_batch().is_some())
        .expect("durable create");
    persist_effect(&mut store, create);
    let edit = one_effect(
        &mut state,
        Action::EditThought {
            thought_id,
            revision_id: ids.revision_id(),
            before_content: content.clone(),
            after_content: format!("{content}!"),
            before_annotations: vec![annotation.clone()],
            after_annotations: Vec::new(),
            before_cursor: TextPosition::new(0, 0),
            after_cursor: TextPosition::new(0, 1),
            at: Timestamp::from_millis(3),
        },
    );
    persist_effect(&mut store, &edit);
    let undo = one_effect(
        &mut state,
        Action::Undo {
            operation_id: ids.operation_id(),
            scope: UndoScope::Editor { thought_id },
            at: Timestamp::from_millis(4),
        },
    );
    persist_effect(&mut store, &undo);
    drop(store);

    let snapshot = fixture.open().load_session(session_id).expect("reopen");
    let restored = snapshot.board.thought(thought_id).expect("thought");
    assert_eq!(restored.content, content);
    assert_eq!(restored.annotations, [annotation]);
    assert_eq!(snapshot.editor_history_cursors, [(thought_id, 0)]);
}

#[test]
fn invocation_reference_projection_survives_reopen_without_rewriting_content() {
    let fixture = DatabaseFixture::new();
    let mut store = fixture.open();
    let mut ids = FakeIdGenerator::new(1_725_000_100_000);
    let mut state = session_state(&mut ids, &test_path("proqi-invocation-reference"));
    let session_id = state.board.session.id;
    store
        .commit(&OperationBatch::CreateSession(state.board.session.clone()))
        .expect("session");
    let thought_id = ids.thought_id();
    let content = "Herdr collaborator: coaching-philipp (claude) at workspace Consulting (w4), tab coaching-philipp (w4:t2), pane w4:p2".to_owned();
    let annotation = ContentAnnotation {
        start: 0,
        end: content.len(),
        kind: ContentAnnotationKind::InvocationReference {
            display_name: "@coaching-philipp · claude".to_owned(),
        },
    };
    let effects = reduce(
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
    let create = effects
        .iter()
        .find(|effect| effect.persistence_batch().is_some())
        .expect("durable create");
    persist_effect(&mut store, create);
    drop(store);

    let snapshot = fixture.open().load_session(session_id).expect("reopen");
    let restored = snapshot.board.thought(thought_id).expect("thought");
    assert_eq!(restored.content, content);
    assert_eq!(restored.annotations, [annotation]);
}
