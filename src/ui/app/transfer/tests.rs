use crate::ui::input::RoutedInput as UiInput;
use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, Effect, FirstRunEnvironment, ThoughtMutation, first_run_board},
    domain::{
        ContentAnnotation, OperationSequence, Session, SessionBoard, Thought, ThoughtPosition,
        Timestamp,
    },
    ports::{
        editor::CursorMovement,
        environment::IdGenerator,
        store::{CommitReceipt, DurableIdentity, SessionHit},
    },
    ui::{BoardApp, UiKey},
};

#[test]
fn transfer_preserves_annotations_and_removes_only_after_destination_receipt() {
    let mut ids = FakeIdGenerator::new(1_725_200_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let source = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-transfer-source"),
        Timestamp::from_millis(1),
    )
    .expect("source session");
    let destination = ids.session_id();
    let mut thought = Thought::new(
        ids.thought_id(),
        source.id,
        "Press Enter".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    thought
        .set_annotations(vec![ContentAnnotation::shortcut(6, 11)])
        .expect("annotation");
    let thought_id = thought.id;
    let board = SessionBoard::new(source, vec![thought.clone()]).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
    assert_eq!(
        app.begin_session_transfer(true, &mut ids, &clock),
        vec![Effect::DiscoverTransferSessions { generation: 1 }]
    );
    assert_loading_input_is_ignored(&mut app, &mut ids, &clock);
    app.complete_transfer_discovery(1, Ok(vec![session_hit(destination)]));
    assert_modified_delete_edits_query(&mut app, &mut ids, &clock);
    let effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let [Effect::TransferThought(request)] = effects.as_slice() else {
        panic!("expected transfer request");
    };
    assert_eq!(request.content, thought.content);
    assert_eq!(request.annotations, thought.annotations);
    assert_eq!(request.source_thought_id, thought_id);
    assert_thought_is_live(&app, thought_id);
    let failed = app.complete_session_transfer(
        request,
        Err("destination unavailable".to_owned()),
        &mut ids,
        &clock,
    );
    assert!(failed.is_empty());
    assert_thought_is_live(&app, thought_id);
    let receipt = CommitReceipt {
        session_id: destination,
        sequence: OperationSequence::new(1),
        identity: DurableIdentity::Operation(request.operation_id),
        idempotent_replay: false,
    };
    let completion = app.complete_session_transfer(
        request,
        Ok(ThoughtMutation {
            thought_id: ids.thought_id(),
            receipt,
        }),
        &mut ids,
        &clock,
    );
    assert!(matches!(
        completion.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert!(app.state.board.live_thoughts().is_empty());
}

#[test]
fn stale_transfer_receipt_keeps_newer_source_content_and_annotations() {
    let mut ids = FakeIdGenerator::new(1_725_202_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let source = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-transfer-stale-source"),
        Timestamp::from_millis(1),
    )
    .expect("source session");
    let destination = ids.session_id();
    let thought = Thought::new(
        ids.thought_id(),
        source.id,
        "sent version".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let thought_id = thought.id;
    let board = SessionBoard::new(source, vec![thought]).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
    app.begin_session_transfer(true, &mut ids, &clock);
    app.complete_transfer_discovery(1, Ok(vec![session_hit(destination)]));
    let effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let [Effect::TransferThought(request)] = effects.as_slice() else {
        panic!("expected transfer request");
    };

    let edit_effects = app.reduce(crate::application::Action::EditThought {
        thought_id,
        revision_id: ids.revision_id(),
        before_content: request.content.clone(),
        after_content: "newer local version".to_owned(),
        before_annotations: request.annotations.clone(),
        after_annotations: Vec::new(),
        before_cursor: crate::domain::TextPosition::new(0, request.content.len()),
        after_cursor: crate::domain::TextPosition::new(0, 19),
        at: Timestamp::from_millis(4),
    });
    assert!(matches!(
        edit_effects.as_slice(),
        [Effect::CommitRevision(_)]
    ));
    let receipt = CommitReceipt {
        session_id: destination,
        sequence: OperationSequence::new(1),
        identity: DurableIdentity::Operation(request.operation_id),
        idempotent_replay: false,
    };

    let completion = app.complete_session_transfer(
        request,
        Ok(ThoughtMutation {
            thought_id: ids.thought_id(),
            receipt,
        }),
        &mut ids,
        &clock,
    );

    assert!(completion.is_empty());
    let current = app
        .state
        .board
        .thought(thought_id)
        .expect("source retained");
    assert_eq!(current.content, "newer local version");
    assert_eq!(
        app.status_text(),
        Some("thought sent; source changed and was kept")
    );
}

#[test]
fn stale_transfer_receipt_keeps_an_annotation_only_source_change() {
    let mut ids = FakeIdGenerator::new(1_725_203_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let source = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-transfer-stale-annotation"),
        Timestamp::from_millis(1),
    )
    .expect("source session");
    let destination = ids.session_id();
    let mut thought = Thought::new(
        ids.thought_id(),
        source.id,
        "Press Enter".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    thought
        .set_annotations(vec![ContentAnnotation::shortcut(6, 11)])
        .expect("annotation");
    let thought_id = thought.id;
    let board = SessionBoard::new(source, vec![thought]).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
    app.begin_session_transfer(true, &mut ids, &clock);
    app.complete_transfer_discovery(1, Ok(vec![session_hit(destination)]));
    let effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let [Effect::TransferThought(request)] = effects.as_slice() else {
        panic!("expected transfer request");
    };
    let edit_effects = app.reduce(crate::application::Action::EditThought {
        thought_id,
        revision_id: ids.revision_id(),
        before_content: request.content.clone(),
        after_content: request.content.clone(),
        before_annotations: request.annotations.clone(),
        after_annotations: Vec::new(),
        before_cursor: crate::domain::TextPosition::new(0, request.content.len()),
        after_cursor: crate::domain::TextPosition::new(0, request.content.len()),
        at: Timestamp::from_millis(4),
    });
    assert!(matches!(
        edit_effects.as_slice(),
        [Effect::CommitRevision(_)]
    ));
    let receipt = CommitReceipt {
        session_id: destination,
        sequence: OperationSequence::new(1),
        identity: DurableIdentity::Operation(request.operation_id),
        idempotent_replay: false,
    };

    let completion = app.complete_session_transfer(
        request,
        Ok(ThoughtMutation {
            thought_id: ids.thought_id(),
            receipt,
        }),
        &mut ids,
        &clock,
    );

    assert!(completion.is_empty());
    let current = app
        .state
        .board
        .thought(thought_id)
        .expect("source retained");
    assert!(current.annotations.is_empty());
}

#[test]
fn tutorial_shortcut_annotations_cross_the_session_transfer_boundary_exactly() {
    let mut ids = FakeIdGenerator::new(1_725_205_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let source = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-tutorial-transfer-source"),
        Timestamp::from_millis(1),
    )
    .expect("source session");
    let board =
        first_run_board(source, &mut ids, FirstRunEnvironment::Standalone).expect("practice board");
    let thought = board.board().live_thoughts()[1].clone();
    let mut app = BoardApp::new(AppState::new(board.board().clone()), RopeEditorFactory);
    app.state.focused_thought = Some(thought.id);

    assert_eq!(
        app.begin_session_transfer(false, &mut ids, &clock),
        vec![Effect::DiscoverTransferSessions { generation: 1 }]
    );
    app.complete_transfer_discovery(1, Ok(vec![session_hit(ids.session_id())]));
    let effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let [Effect::TransferThought(request)] = effects.as_slice() else {
        panic!("expected transfer request");
    };
    assert_eq!(request.content, thought.content);
    assert_eq!(request.annotations, thought.annotations);
}

#[test]
fn stale_discovery_cannot_mutate_a_reopened_transfer_owner() {
    let mut ids = FakeIdGenerator::new(1_725_207_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let source = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-transfer-generation"),
        Timestamp::from_millis(1),
    )
    .expect("source session");
    let first = Thought::new(
        ids.thought_id(),
        source.id,
        "first".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let second = Thought::new(
        ids.thought_id(),
        source.id,
        "second".to_owned(),
        ThoughtPosition::new(1),
        Timestamp::from_millis(1),
    );
    let second_id = second.id;
    let board = SessionBoard::new(source, vec![first, second]).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);

    assert!(matches!(
        app.begin_session_transfer(false, &mut ids, &clock)
            .as_slice(),
        [Effect::DiscoverTransferSessions { generation: 1 }]
    ));
    app.handle_transfer_input(&UiInput::Key(UiKey::Character('o')), &mut ids, &clock);
    app.handle_transfer_input(&UiInput::Key(UiKey::Escape), &mut ids, &clock);
    app.state.focused_thought = Some(second_id);
    assert!(matches!(
        app.begin_session_transfer(true, &mut ids, &clock)
            .as_slice(),
        [Effect::DiscoverTransferSessions { generation: 2 }]
    ));
    app.handle_transfer_input(&UiInput::Key(UiKey::Character('n')), &mut ids, &clock);

    app.complete_transfer_discovery(1, Ok(vec![session_hit(ids.session_id())]));
    app.complete_transfer_discovery(1, Ok(Vec::new()));
    app.complete_transfer_discovery(1, Err(crate::ports::store::StoreError::Busy));
    let current = app.transfer.as_ref().expect("reopened transfer owner");
    assert_eq!(current.generation, 2);
    assert_eq!(current.source_thought_id, second_id);
    assert!(current.remove_source);
    assert!(current.loading);
    assert_eq!(current.query.text(), "n");

    app.complete_transfer_discovery(2, Ok(vec![session_hit(ids.session_id())]));
    let current = app.transfer.as_ref().expect("current completion");
    assert!(!current.loading);
    assert_eq!(current.sessions.len(), 1);
}

fn assert_modified_delete_edits_query(
    app: &mut BoardApp,
    ids: &mut FakeIdGenerator,
    clock: &FakeClock,
) {
    for character in "hx".chars() {
        app.handle_transfer_input(&UiInput::Key(UiKey::Character(character)), ids, clock);
    }
    app.handle_transfer_input(
        &UiInput::Key(UiKey::Move {
            movement: CursorMovement::GraphemeBack,
            extend_selection: false,
        }),
        ids,
        clock,
    );
    app.handle_transfer_input(&UiInput::Key(UiKey::ModifiedDelete), ids, clock);
    assert_eq!(app.transfer_view().expect("transfer").0, "h");
    app.handle_transfer_input(&UiInput::Key(UiKey::Backspace), ids, clock);
}

fn assert_loading_input_is_ignored(
    app: &mut BoardApp,
    ids: &mut FakeIdGenerator,
    clock: &FakeClock,
) {
    let effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), ids, clock);
    assert!(effects.is_empty());
    assert!(app.transfer_view().is_some());
}

fn assert_thought_is_live(app: &BoardApp, thought_id: crate::domain::ThoughtId) {
    assert!(
        app.state
            .board
            .thought(thought_id)
            .is_some_and(Thought::is_live)
    );
}

fn session_hit(id: crate::domain::SessionId) -> SessionHit {
    SessionHit {
        id,
        name: Some("destination".to_owned()),
        origin_cwd: std::env::temp_dir(),
        last_opened_cwd: std::env::temp_dir(),
        last_opened_at: Timestamp::from_millis(1),
        last_active_at: Timestamp::from_millis(1),
        thought_count: 0,
        excerpt: String::new(),
        previews: Vec::new(),
        search_content: String::new(),
        integration_context: None,
        trashed: false,
    }
}
