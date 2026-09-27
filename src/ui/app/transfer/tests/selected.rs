use super::*;
use crate::domain::{BoardItemId, Separator, SessionId, ThoughtId};

fn selected_transfer_app() -> (
    BoardApp,
    FakeIdGenerator,
    FakeClock,
    SessionId,
    [ThoughtId; 3],
    crate::domain::SeparatorId,
) {
    let mut ids = FakeIdGenerator::new(1_725_208_000_000);
    let clock = FakeClock::new(Timestamp::from_millis(3));
    let source = Session::new(
        ids.session_id(),
        std::env::temp_dir().join("proqi-selected-transfer"),
        Timestamp::from_millis(1),
    )
    .expect("source");
    let destination = ids.session_id();
    let first = Thought::new(
        ids.thought_id(),
        source.id,
        "first".to_owned(),
        ThoughtPosition::new(0),
        Timestamp::from_millis(1),
    );
    let separator = Separator::new(
        ids.separator_id(),
        source.id,
        ThoughtPosition::new(1),
        Timestamp::from_millis(1),
    );
    let second = Thought::new(
        ids.thought_id(),
        source.id,
        "second".to_owned(),
        ThoughtPosition::new(2),
        Timestamp::from_millis(1),
    );
    let third = Thought::new(
        ids.thought_id(),
        source.id,
        "third".to_owned(),
        ThoughtPosition::new(3),
        Timestamp::from_millis(1),
    );
    let mut app = BoardApp::new(
        AppState::new(
            SessionBoard::with_separators(
                source,
                vec![first.clone(), second.clone(), third.clone()],
                vec![separator.clone()],
            )
            .expect("board"),
        ),
        RopeEditorFactory,
    );
    app.state.focused_item = Some(first.id.into());
    app.replace_board_selection([
        BoardItemId::Thought(first.id),
        BoardItemId::Separator(separator.id),
        BoardItemId::Thought(second.id),
    ]);
    (
        app,
        ids,
        clock,
        destination,
        [first.id, second.id, third.id],
        separator.id,
    )
}

#[test]
fn selected_send_and_remove_waits_for_the_complete_destination_cohort() {
    let (mut app, mut ids, clock, destination, [first, second, third], separator) =
        selected_transfer_app();
    assert!(matches!(
        app.begin_session_transfer(true, &mut ids, &clock)
            .as_slice(),
        [Effect::DiscoverTransferSessions { .. }]
    ));
    app.complete_transfer_discovery(1, Ok(vec![session_hit(destination)]));
    let transfer_effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let [Effect::TransferThoughts(request)] = transfer_effects.as_slice() else {
        panic!("expected selected transfer");
    };
    let request = request.clone();
    assert_eq!(
        request
            .items
            .iter()
            .map(|item| item.source_thought_id)
            .collect::<Vec<_>>(),
        vec![first, second]
    );
    assert_sources_live(&app, first, second);
    assert!(
        app.complete_session_transfer_batch(
            &request,
            Err("destination busy".to_owned()),
            &mut ids,
            &clock
        )
        .is_empty()
    );
    assert_sources_live(&app, first, second);

    assert!(matches!(
        app.begin_session_transfer(true, &mut ids, &clock)
            .as_slice(),
        [Effect::DiscoverTransferSessions { .. }]
    ));
    app.complete_transfer_discovery(2, Ok(vec![session_hit(destination)]));
    let retry_effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let [Effect::TransferThoughts(retry)] = retry_effects.as_slice() else {
        panic!("expected retry");
    };
    assert_eq!(retry, &request);
    let receipt = CommitReceipt {
        session_id: destination,
        sequence: OperationSequence::new(1),
        identity: DurableIdentity::Operation(request.operation_id),
        idempotent_replay: false,
    };
    let effects = app.complete_session_transfer_batch(&request, Ok(receipt), &mut ids, &clock);
    let [
        Effect::FinishTransfer {
            removal: Some(operation),
            ..
        },
    ] = effects.as_slice()
    else {
        panic!("expected one atomic source removal");
    };
    assert_eq!(operation.id, request.removal_operation_id);
    assert_eq!(operation.kind, BoardOperationKind::TransferAndRemove);
    assert!(!app.state.board.thought(first).is_some_and(Thought::is_live));
    assert!(
        !app.state
            .board
            .thought(second)
            .is_some_and(Thought::is_live)
    );
    assert!(app.state.board.thought(third).is_some_and(Thought::is_live));
    assert!(
        app.state
            .board
            .separator(separator)
            .is_some_and(Separator::is_live)
    );
}

fn assert_sources_live(app: &BoardApp, first: ThoughtId, second: ThoughtId) {
    assert!(app.state.board.thought(first).is_some_and(Thought::is_live));
    assert!(
        app.state
            .board
            .thought(second)
            .is_some_and(Thought::is_live)
    );
}

#[test]
fn one_selected_thought_with_separator_uses_recoverable_cohort() {
    let (mut app, mut ids, clock, destination, [first, second, _], separator) =
        selected_transfer_app();
    app.replace_board_selection([
        BoardItemId::Thought(first),
        BoardItemId::Separator(separator),
    ]);
    app.begin_session_transfer(true, &mut ids, &clock);
    app.complete_transfer_discovery(1, Ok(vec![session_hit(destination)]));
    let effects = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    let [Effect::TransferThoughts(request)] = effects.as_slice() else {
        panic!("mixed selection must use durable cohort");
    };
    assert_eq!(request.items.len(), 1);
    assert_eq!(request.items[0].source_thought_id, first);
    let request = request.clone();
    app.complete_session_transfer_batch(&request, Err("interrupted".to_owned()), &mut ids, &clock);
    assert_sources_live(&app, first, second);
    app.begin_session_transfer(true, &mut ids, &clock);
    app.complete_transfer_discovery(2, Ok(vec![session_hit(destination)]));
    let retry = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(matches!(retry.as_slice(), [Effect::TransferThoughts(value)] if value == &request));
    let receipt = CommitReceipt {
        session_id: destination,
        sequence: OperationSequence::new(1),
        identity: DurableIdentity::Operation(request.operation_id),
        idempotent_replay: false,
    };
    let completion = app.complete_session_transfer_batch(&request, Ok(receipt), &mut ids, &clock);
    assert!(matches!(
        completion.as_slice(),
        [Effect::FinishTransfer {
            removal: Some(_),
            ..
        }]
    ));
    assert!(!app.state.board.thought(first).is_some_and(Thought::is_live));
    assert!(
        app.state
            .board
            .thought(second)
            .is_some_and(Thought::is_live)
    );
    assert!(
        app.state
            .board
            .separator(separator)
            .is_some_and(Separator::is_live)
    );
}

#[test]
fn pending_single_transfer_blocks_its_thought_but_not_an_unrelated_selected_run() {
    let (mut app, mut ids, clock, destination, [first, _second, third], separator) =
        selected_transfer_app();
    app.replace_board_selection(std::iter::empty::<BoardItemId>());
    app.state.focused_item = Some(third.into());
    app.begin_session_transfer(true, &mut ids, &clock);
    app.complete_transfer_discovery(1, Ok(vec![session_hit(destination)]));
    let pending = app.handle_transfer_input(&UiInput::Key(UiKey::Enter), &mut ids, &clock);
    assert!(matches!(pending.as_slice(), [Effect::TransferThoughts(_)]));
    assert!(app.reorder(&mut ids, &clock, -1).is_empty());
    assert!(app.reflow_thought_in_place(&mut ids, &clock).is_empty());
    assert!(
        app.begin_session_transfer(false, &mut ids, &clock)
            .is_empty()
    );

    app.replace_board_selection([
        BoardItemId::Thought(first),
        BoardItemId::Separator(separator),
    ]);
    app.state.focused_item = Some(third.into());
    let moved = app.reorder(&mut ids, &clock, 1);
    assert!(matches!(
        moved.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(app.state.focused_item, Some(third.into()));
    assert!(app.item_selected(first.into()));
    assert!(app.item_selected(separator.into()));
}
