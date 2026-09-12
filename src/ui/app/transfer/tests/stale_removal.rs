use crate::{
    adapters::{
        editor::RopeEditorFactory,
        memory::{FakeClock, FakeIdGenerator},
    },
    application::{AppState, ThoughtMutation},
    domain::{OperationSequence, Session, SessionBoard, Thought, ThoughtPosition, Timestamp},
    ports::{
        environment::IdGenerator,
        store::{CommitReceipt, DurableIdentity},
        transfer::SessionTransferRequest,
    },
    ui::{BoardApp, UiInput, UiKey},
};

#[test]
fn pending_remove_transfer_locks_commands_and_stale_success_keeps_the_source() {
    let mut ids = FakeIdGenerator::new(1_725_210_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-transfer-stale-source"),
        Timestamp::from_millis(1),
    )
    .expect("source session");
    let destination = ids.session_id();
    let thought = Thought::new(
        ids.thought_id(),
        session.id,
        "original".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let thought_id = thought.id;
    let operation_id = ids.operation_id();
    let request = SessionTransferRequest {
        destination_session_id: destination,
        source_thought_id: thought_id,
        operation_id,
        content: thought.content.clone(),
        annotations: thought.annotations.clone(),
        remove_source: true,
    };
    let board = SessionBoard::new(session, vec![thought]).expect("board");
    let mut app = BoardApp::new(AppState::new(board), RopeEditorFactory);
    app.pending_transfer_removals
        .insert(operation_id, thought_id);

    app.open_palette();
    for character in "edit thought".chars() {
        let _effects = app.handle(UiInput::Key(UiKey::Character(character)), &mut ids, &clock);
    }
    let edit = app
        .command_palette_view()
        .expect("Commands")
        .rows
        .into_iter()
        .find(|row| row.primary == "Edit thought")
        .expect("Edit command");
    assert!(!edit.enabled);
    assert_eq!(
        edit.secondary.as_deref(),
        Some("Thought has an operation in progress")
    );

    app.state
        .board
        .thought_mut(thought_id)
        .expect("source thought")
        .content
        .push_str(" changed");
    let receipt = CommitReceipt {
        session_id: destination,
        sequence: OperationSequence::new(1),
        identity: DurableIdentity::Operation(operation_id),
        idempotent_replay: false,
    };
    let effects = app.complete_session_transfer(
        &request,
        Ok(ThoughtMutation {
            thought_id: ids.thought_id(),
            receipt,
        }),
        &mut ids,
        &clock,
    );

    assert!(effects.is_empty());
    assert!(
        app.state
            .board
            .thought(thought_id)
            .is_some_and(Thought::is_live)
    );
    assert_eq!(
        app.status_text(),
        Some("thought changed after it was sent; source kept")
    );
}
