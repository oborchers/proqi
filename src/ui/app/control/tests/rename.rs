use super::*;

#[test]
fn pending_owner_rename_rejects_reordering_and_clears_after_failure() {
    let mut ids = FakeIdGenerator::new(1_725_230_000_000);
    let mut session = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-control-rename-order"),
        Timestamp::from_millis(1),
    )
    .expect("session");
    session.name = Some("Durable".to_owned());
    let mut app = BoardApp::new(
        AppState::new(SessionBoard::new(session, Vec::new()).expect("board")),
        crate::adapters::editor::RopeEditorFactory,
    );
    let clock = FakeClock::new(Timestamp::from_millis(2));
    let first = ControlMutation::RenameSession {
        operation_id: ids.operation_id(),
        name: Some("First".to_owned()),
    };

    let effects = app.handle_control(&first, &clock).expect("first rename");
    assert!(matches!(
        effects.as_slice(),
        [Effect::CommitBrowserOperation(_)]
    ));
    assert_eq!(app.state.board.session.name.as_deref(), Some("First"));

    let second = ControlMutation::RenameSession {
        operation_id: ids.operation_id(),
        name: Some("Second".to_owned()),
    };
    assert_eq!(
        app.handle_control(&second, &clock),
        Err(ApplicationError::InvalidState)
    );
    assert_eq!(app.state.board.session.name.as_deref(), Some("First"));

    app.complete_session_rename(
        Some("Durable".to_owned()),
        Err(crate::ports::store::StoreError::Busy),
    );
    assert_eq!(app.state.board.session.name.as_deref(), Some("Durable"));

    let retry = app.handle_control(&second, &clock).expect("retry rename");
    assert!(matches!(
        retry.as_slice(),
        [Effect::CommitBrowserOperation(_)]
    ));
    assert_eq!(app.state.board.session.name.as_deref(), Some("Second"));
}
