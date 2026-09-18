use super::*;
use proqi::{
    adapters::editor::RopeEditorFactory,
    application::{AppState, InteractionMode},
    domain::{BoardItemId, Direction, Separator, Thought, ThoughtPosition},
    ports::{
        agent::{
            AgentDeliveryCapabilities, AgentError, AgentSessionBinding, AgentState, AgentTarget,
            HarnessKind, HerdrAgentAddress, PaneContext, PaneRect, SubmissionReceipt,
        },
        store::StoreError,
    },
};

#[path = "separators/review.rs"]
mod review;

struct Mixed {
    fixture: Fixture,
    first: proqi::domain::ThoughtId,
    separator: proqi::domain::SeparatorId,
    second: proqi::domain::ThoughtId,
}

impl Mixed {
    fn new() -> Self {
        let mut ids = FakeIdGenerator::new(1_725_990_000_000);
        let now = Timestamp::from_millis(1);
        let session = Session::new(
            ids.session_id(),
            std::env::temp_dir().join("proqi-separator-ui"),
            now,
        )
        .expect("session");
        let first = ids.thought_id();
        let separator = ids.separator_id();
        let second = ids.thought_id();
        let thoughts = vec![
            Thought::new(
                first,
                session.id,
                "first".to_owned(),
                ThoughtPosition::new(0),
                now,
            ),
            Thought::new(
                second,
                session.id,
                "second".to_owned(),
                ThoughtPosition::new(2),
                now,
            ),
        ];
        let separators = vec![Separator::new(
            separator,
            session.id,
            ThoughtPosition::new(1),
            now,
        )];
        let mut state = AppState::new(
            SessionBoard::with_separators(session, thoughts, separators).expect("board"),
        );
        state.mode = InteractionMode::Board;
        state.focused_item = Some(separator.into());
        Self {
            fixture: Fixture {
                app: BoardApp::new(state, RopeEditorFactory),
                ids,
                clock: FakeClock::new(Timestamp::from_millis(2)),
            },
            first,
            separator,
            second,
        }
    }

    fn ids(&self) -> Vec<BoardItemId> {
        self.fixture
            .app
            .state
            .board
            .live_items()
            .into_iter()
            .map(proqi::domain::BoardItemRef::id)
            .collect()
    }

    fn select_all(&mut self) {
        self.fixture.input(crate::key_input(UiKey::SelectAll));
    }
}

fn open_command(fixture: &mut Fixture, query: &str) -> Vec<Effect> {
    fixture.input(crate::key_input(UiKey::Character(':')));
    for character in query.chars() {
        fixture.input(crate::key_input(UiKey::Character(character)));
    }
    fixture.effects(crate::key_input(UiKey::Enter))
}

fn target() -> AgentTarget {
    let source = PaneContext {
        workspace_id: "w1".to_owned(),
        tab_id: "w1:t1".to_owned(),
        pane_id: "w1:p1".to_owned(),
        rect: PaneRect {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        },
    };
    let address = HerdrAgentAddress::new(
        "w1".to_owned(),
        "w1:t1".to_owned(),
        "w1:p2".to_owned(),
        HarnessKind::new("codex").expect("harness"),
        AgentSessionBinding::established("session-p2").expect("binding"),
    )
    .expect("address");
    AgentTarget::adjacent(
        "herdr".to_owned(),
        19,
        Direction::Right,
        address,
        "codex w1:p2".to_owned(),
        AgentState::Idle,
        AgentDeliveryCapabilities::SUBMIT_ONLY,
        PaneRect {
            x: 20,
            y: 0,
            width: 20,
            height: 20,
        },
        source,
    )
}

fn start_submission(mixed: &mut Mixed) -> proqi::ports::agent::SubmissionRequest {
    mixed
        .fixture
        .app
        .complete_agent_discovery(Ok(vec![target()]));
    let prepared = mixed
        .fixture
        .effects(crate::key_input(UiKey::Character('s')));
    let [Effect::PrepareSubmission(attempt)] = prepared.as_slice() else {
        panic!("prepared submission");
    };
    let sending = mixed
        .fixture
        .app
        .complete_submission_prepared(attempt.id, Ok(()));
    assert!(matches!(
        sending.as_slice(),
        [Effect::MarkSubmissionSending { .. }]
    ));
    let submitted = mixed
        .fixture
        .app
        .complete_submission_sending(attempt.id, Ok(()));
    let [Effect::SubmitAgent(request)] = submitted.as_slice() else {
        panic!("submitted request");
    };
    request.clone()
}

#[test]
fn commands_insert_separator_on_empty_board_and_normal_creation_stays_distinct() {
    let mut fixture = Fixture::new();
    fixture.input(crate::key_input(UiKey::Escape));
    let inserted = open_command(&mut fixture, "insert separator");
    let [Effect::CommitBoardOperation(operation)] = inserted.as_slice() else {
        panic!("separator insertion");
    };
    fixture
        .app
        .acknowledge_persistence(operation.sequence, true);
    let separator = fixture.app.state.focused_item.expect("separator focus");
    assert!(matches!(separator, BoardItemId::Separator(_)));
    assert!(fixture.app.state.board.live_thoughts().is_empty());

    let created = fixture.effects(crate::key_input(UiKey::Character('n')));
    let [Effect::CommitBoardOperation(operation)] = created.as_slice() else {
        panic!("blank thought creation");
    };
    fixture
        .app
        .acknowledge_persistence(operation.sequence, true);
    let items = fixture.app.state.board.live_items();
    assert_eq!(items[0].id(), separator);
    let BoardItemId::Thought(blank) = items[1].id() else {
        panic!("blank thought");
    };
    assert_eq!(
        fixture
            .app
            .state
            .board
            .thought(blank)
            .expect("blank")
            .content,
        ""
    );
}

#[test]
fn separator_copy_cut_submit_and_paste_paths_never_treat_it_as_text() {
    let mut mixed = Mixed::new();
    assert!(
        mixed
            .fixture
            .effects(crate::key_input(UiKey::Copy))
            .is_empty()
    );
    assert_eq!(
        mixed.fixture.app.status_text(),
        Some("separator has no text to copy or cut")
    );
    assert!(
        mixed
            .fixture
            .effects(crate::key_input(UiKey::Cut))
            .is_empty()
    );
    assert!(
        mixed
            .fixture
            .effects(crate::key_input(UiKey::Character('s')))
            .is_empty()
    );
    assert_eq!(
        mixed.fixture.app.status_text(),
        Some("separator has no text to submit")
    );

    let pasted = mixed.fixture.effects(UiInput::Paste("pasted".to_owned()));
    assert!(matches!(
        pasted.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    let items = mixed.ids();
    assert_eq!(items[0], mixed.first.into());
    assert_eq!(items[1], mixed.separator.into());
    let BoardItemId::Thought(pasted_id) = items[2] else {
        panic!("pasted thought");
    };
    assert_eq!(
        mixed
            .fixture
            .app
            .state
            .board
            .thought(pasted_id)
            .expect("pasted")
            .content,
        "pasted"
    );
}

#[test]
fn mixed_copy_and_cut_filter_text_and_preserve_separator_selection() {
    let mut mixed = Mixed::new();
    mixed.select_all();
    let copy = mixed.fixture.effects(crate::key_input(UiKey::Copy));
    let [
        Effect::WriteClipboard {
            request_id: copy_request,
            content,
            annotations,
            ..
        },
    ] = copy.as_slice()
    else {
        panic!("copy request");
    };
    assert_eq!(content, "first\n\nsecond");
    assert!(annotations.is_empty());
    assert!(
        mixed
            .fixture
            .app
            .complete_clipboard_write(
                *copy_request,
                Ok(()),
                &mut mixed.fixture.ids,
                &mixed.fixture.clock,
            )
            .is_empty()
    );

    let cut = mixed.fixture.effects(crate::key_input(UiKey::Cut));
    let [Effect::WriteClipboard { request_id, .. }] = cut.as_slice() else {
        panic!("cut request");
    };
    let completed = mixed.fixture.app.complete_clipboard_write(
        *request_id,
        Ok(()),
        &mut mixed.fixture.ids,
        &mixed.fixture.clock,
    );
    assert!(matches!(
        completed.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(mixed.ids(), vec![mixed.separator.into()]);
    assert!(mixed.fixture.app.item_selected(mixed.separator.into()));
}

#[test]
fn failed_mixed_cut_and_hover_leave_items_and_selection_unchanged() {
    let mut mixed = Mixed::new();
    mixed.select_all();
    let cut = mixed.fixture.effects(crate::key_input(UiKey::Cut));
    let [Effect::WriteClipboard { request_id, .. }] = cut.as_slice() else {
        panic!("cut request");
    };
    assert!(matches!(
        mixed
            .fixture
            .app
            .complete_clipboard_write(
                *request_id,
                Err(FailureCode::ClipboardFailed),
                &mut mixed.fixture.ids,
                &mixed.fixture.clock,
            )
            .as_slice(),
        [Effect::Notify {
            code: FailureCode::ClipboardFailed
        }]
    ));
    let layout = mixed.fixture.app.prepare_frame(Rect::new(0, 0, 40, 12));
    let line = layout.separators[0].line.expect("visible separator line");
    let column = line.x.saturating_add(2);
    assert!(matches!(
        layout.hit_test(column, line.y),
        Some(HitTarget::Separator(id)) if id == mixed.separator
    ));
    mixed.fixture.input(UiInput::Pointer(PointerInput {
        column,
        row: line.y,
        kind: PointerKind::Move,
        extend_selection: false,
    }));
    assert_eq!(mixed.ids().len(), 3);
    assert!(mixed.fixture.app.item_selected(mixed.separator.into()));
    let hovered = mixed.fixture.app.hovered();
    assert!(
        matches!(
        hovered,
        Some(HitTarget::Separator(id) | HitTarget::SeparatorDragHandle(id))
            if id == mixed.separator
        ),
        "unexpected separator hover target: {hovered:?}"
    );
}

#[test]
fn keyboard_range_reorder_duplicate_and_typing_treat_separator_as_an_item() {
    let mut mixed = Mixed::new();
    mixed.fixture.input(crate::key_input(UiKey::Move {
        movement: CursorMovement::VisualDown,
        extend_selection: true,
    }));
    assert!(mixed.fixture.app.item_selected(mixed.separator.into()));
    assert!(mixed.fixture.app.item_selected(mixed.second.into()));

    mixed.fixture.input(crate::key_input(UiKey::Escape));
    mixed.fixture.input(crate::key_input(UiKey::Character('k')));
    let moved = mixed
        .fixture
        .effects(crate::key_input(UiKey::PrimaryShiftMove {
            movement: CursorMovement::VisualDown,
        }));
    assert!(matches!(
        moved.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(
        mixed.ids(),
        vec![
            mixed.first.into(),
            mixed.second.into(),
            mixed.separator.into()
        ]
    );

    let duplicated = mixed.fixture.effects(crate::key_input(UiKey::Duplicate));
    assert!(matches!(
        duplicated.as_slice(),
        [Effect::CommitBoardOperation(_)]
    ));
    assert_eq!(mixed.fixture.app.state.board.live_separators().len(), 2);
    let before = mixed.fixture.app.state.board.clone();
    mixed
        .fixture
        .input(UiInput::KeyStroke(KeyStroke::press(LogicalKey::Character(
            'ø',
        ))));
    assert_eq!(mixed.fixture.app.state.board, before);
}

#[test]
fn separator_refuses_edit_collapse_transform_and_transfer_actions() {
    let mut mixed = Mixed::new();
    let before = mixed.fixture.app.state.board.clone();
    for key in [
        UiKey::Enter,
        UiKey::Character('c'),
        UiKey::Character('f'),
        UiKey::Character('t'),
    ] {
        assert!(mixed.fixture.effects(crate::key_input(key)).is_empty());
        assert_eq!(mixed.fixture.app.state.mode, InteractionMode::Board);
        assert_eq!(mixed.fixture.app.state.board, before);
    }

    let mut transfer = Mixed::new();
    assert!(open_command(&mut transfer.fixture, "send to another proqi session").is_empty());
    assert_eq!(transfer.fixture.app.state.board, before);
}

#[test]
fn mixed_submission_filters_separator_across_failure_retry_and_unknown_acceptance() {
    let mut failed = Mixed::new();
    failed.select_all();
    let failed_request = start_submission(&mut failed);
    assert_eq!(failed_request.content, "first\n\nsecond");
    let journal = failed
        .fixture
        .app
        .complete_submission(failed_request.submission_id, Err(AgentError::TimedOut));
    assert!(matches!(
        journal.as_slice(),
        [Effect::FinishSubmission { removal: None, .. }]
    ));
    assert!(
        failed
            .fixture
            .app
            .complete_submission_journaled(failed_request.submission_id, Ok(()))
            .is_empty()
    );
    assert_eq!(failed.ids().len(), 3);

    let mut accepted = Mixed::new();
    accepted.select_all();
    let request = start_submission(&mut accepted);
    let journal = accepted.fixture.app.complete_submission(
        request.submission_id,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: request.target,
            post_state: Some(AgentState::Unknown),
        }),
    );
    let [
        Effect::FinishSubmission {
            removal: Some(operation),
            ..
        },
    ] = journal.as_slice()
    else {
        panic!("accepted removal");
    };
    let sequence = operation.sequence;
    accepted
        .fixture
        .app
        .acknowledge_persistence(sequence, false);
    accepted
        .fixture
        .app
        .submission_persistence_failed(request.submission_id, &StoreError::DiskFull);
    assert_eq!(accepted.ids().len(), 3);
    assert!(
        accepted
            .fixture
            .app
            .state
            .board
            .separator(accepted.separator)
            .is_some_and(Separator::is_live)
    );

    accepted.fixture.app.acknowledge_persistence(sequence, true);
    accepted
        .fixture
        .app
        .complete_submission_journaled(request.submission_id, Ok(()));
    assert_eq!(accepted.ids(), vec![accepted.separator.into()]);
}
