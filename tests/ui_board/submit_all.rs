use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{AgentError, AgentState, SubmissionDisposition, SubmissionReceipt},
};

fn populate_exact_board(fixture: &mut Fixture) {
    fixture.paste("/goal first");
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Character('n')));
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.paste("/plan Grüße 👩‍💻\r\n第二行");
    fixture.input(UiInput::Key(UiKey::Escape));
}

fn execute_palette(fixture: &mut Fixture, query: &str) -> Vec<Effect> {
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in query.chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    fixture.effects(UiInput::Key(UiKey::Enter))
}

fn click_palette(fixture: &mut Fixture, query: &str) -> Vec<Effect> {
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in query.chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 70, 12));
    let item = layout.overlay.expect("palette overlay").items[0];
    fixture.effects(UiInput::Pointer(PointerInput {
        column: item.x,
        row: item.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }))
}

#[test]
fn palette_submission_labels_use_one_concise_vocabulary() {
    let mut fixture = Fixture::new();
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Left, "w1:p2")]));
    fixture.input(UiInput::Key(UiKey::Character(':')));
    for character in "submit".chars() {
        fixture.input(UiInput::Key(UiKey::Character(character)));
    }

    let (_, entries, selected) = fixture.app.palette_view().expect("palette");
    assert_eq!(
        entries,
        vec![
            "Submit",
            "Submit and keep",
            "Submit all",
            "Submit all and keep",
        ]
    );
    assert_eq!(selected, 0);
}

#[test]
fn palette_submit_all_keep_and_remove_share_one_exact_ordered_request() {
    let mut fixture = Fixture::new();
    populate_exact_board(&mut fixture);
    fixture.input(UiInput::Key(UiKey::Character('k')));
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    let selected = fixture.app.state.focused_thought.expect("selected source");
    let target = super::agent::target(Direction::Left, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let keeping = execute_palette(&mut fixture, "submit all and keep");
    let [Effect::PrepareSubmission(attempt)] = keeping.as_slice() else {
        panic!("expected exactly one prepared submission");
    };
    assert_eq!(attempt.sources.len(), 3);
    assert_eq!(attempt.disposition, SubmissionDisposition::Keep);
    let request = super::agent::start_submission(&mut fixture, &keeping);
    assert_eq!(request.content, "/goal first\n\n\n\nGrüße 👩‍💻\r\n第二行");
    let kept = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: target.clone(),
            post_state: Some(AgentState::Working),
        }),
    );
    assert!(matches!(
        kept.as_slice(),
        [Effect::StoreIntegrationContext { .. }]
    ));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
    assert!(fixture.app.thought_selected(selected));

    let removing = click_palette(&mut fixture, "submit all");
    let request = super::agent::start_submission(&mut fixture, &removing);
    assert_eq!(request.content, "/goal first\n\n\n\nGrüße 👩‍💻\r\n第二行");
    let removed = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target,
            post_state: Some(AgentState::Done),
        }),
    );
    assert!(matches!(
        removed.as_slice(),
        [Effect::StoreIntegrationContext { .. }, Effect::CommitBoardOperation(operation)]
            if operation.kind == proqi::domain::BoardOperationKind::SubmitAndRemove
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());

    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 3);
}

#[test]
fn select_all_then_each_submit_key_addresses_the_complete_board() {
    for (key, disposition) in [
        ('s', SubmissionDisposition::RemoveAfterSuccess),
        ('S', SubmissionDisposition::Keep),
    ] {
        let mut fixture = Fixture::new();
        for content in ["first", "second", "third"] {
            fixture.paste(content);
            fixture.input(UiInput::Key(UiKey::Escape));
        }
        fixture
            .app
            .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Right, "w1:p2")]));

        fixture.input(UiInput::Key(UiKey::Character('a')));
        let effects = fixture.effects(UiInput::Key(UiKey::Character(key)));
        let [Effect::PrepareSubmission(attempt)] = effects.as_slice() else {
            panic!("expected one submission attempt");
        };
        assert_eq!(attempt.disposition, disposition);
        assert_eq!(attempt.sources.len(), 3);
        let request = super::agent::start_submission(&mut fixture, &effects);
        assert_eq!(request.content, "first\n\nsecond\n\nthird");
    }
}

#[test]
fn ambiguous_direction_keeps_selection_stable_through_pointer_and_resize() {
    let mut fixture = Fixture::new();
    for content in ["first", "second"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    let selected = fixture.app.state.focused_thought.expect("selected thought");
    let left = super::agent::target(Direction::Left, "w1:p2");
    let right = super::agent::target(Direction::Right, "w1:p3");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![left, right.clone()]));

    assert!(execute_palette(&mut fixture, "submit all and keep").is_empty());
    let first_layout = fixture.app.prepare_frame(Rect::new(0, 0, 80, 12));
    let other = first_layout.thoughts[0].text_area;
    fixture.input(UiInput::Pointer(PointerInput {
        column: other.x,
        row: other.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    assert!(fixture.app.thought_selected(selected));
    assert_eq!(
        fixture
            .app
            .state
            .board
            .live_thoughts()
            .into_iter()
            .filter(|thought| fixture.app.thought_selected(thought.id))
            .count(),
        1
    );

    fixture.input(UiInput::Resize {
        width: 42,
        height: 9,
    });
    let resized = fixture.app.prepare_frame(Rect::new(0, 0, 42, 9));
    let (_, control) = resized
        .controls
        .iter()
        .find(|(target, _)| {
            *target == proqi::ui::HitTarget::Deliver(Direction::Right, SubmissionDisposition::Keep)
        })
        .expect("right direction control after resize");
    let effects = fixture.effects(UiInput::Pointer(PointerInput {
        column: control.x,
        row: control.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    let request = super::agent::start_submission(&mut fixture, &effects);
    assert_eq!(request.target, right);
    assert_eq!(request.content, "first\n\nsecond");
    assert!(fixture.app.thought_selected(selected));
}

#[test]
fn all_submit_failures_and_empty_boards_are_non_destructive() {
    let mut empty = Fixture::new();
    empty
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Left, "w1:p2")]));
    assert!(execute_palette(&mut empty, "submit all and keep").is_empty());
    assert_eq!(
        empty.app.status_text(),
        Some("board is empty; nothing submitted")
    );

    let mut fixture = Fixture::new();
    for content in ["first", "second"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Left, "w1:p2")]));
    let failures = [
        AgentError::Rejected {
            code: "payload_too_large".to_owned(),
            message: "request exceeds provider limit".to_owned(),
        },
        AgentError::Ambiguous("target changed during revalidation".to_owned()),
        AgentError::Process("transport closed".to_owned()),
    ];
    for failure in failures {
        let effects = execute_palette(&mut fixture, "submit all");
        let request = super::agent::start_submission(&mut fixture, &effects);
        let completion = super::agent::finish_submission(&mut fixture, &request, Err(failure));
        assert!(completion.is_empty());
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
    }
}

#[test]
fn target_change_during_direction_choice_sends_nothing_and_preserves_selection() {
    let mut fixture = Fixture::new();
    for content in ["first", "second"] {
        fixture.paste(content);
        fixture.input(UiInput::Key(UiKey::Escape));
    }
    fixture.input(UiInput::Key(UiKey::Character(' ')));
    let selected = fixture.app.state.focused_thought.expect("selected thought");
    fixture.app.complete_agent_discovery(Ok(vec![
        super::agent::target(Direction::Left, "w1:p2"),
        super::agent::target(Direction::Right, "w1:p3"),
    ]));
    assert!(execute_palette(&mut fixture, "submit all and keep").is_empty());

    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Up, "w1:p4")]));

    assert_eq!(fixture.app.submission_mode(), None);
    assert!(fixture.app.thought_selected(selected));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}
