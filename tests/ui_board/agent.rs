use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{
        AgentDeliveryCapabilities, AgentError, AgentSessionBinding, AgentState, AgentTarget,
        CODEX_AGENT_KIND, HarnessKind, PaneContext, PaneRect, SubmissionDisposition,
        SubmissionReceipt,
    },
};

pub(super) fn target(direction: Direction, pane_id: &str) -> AgentTarget {
    target_with_kind(direction, pane_id, CODEX_AGENT_KIND)
}

pub(super) fn target_with_kind(direction: Direction, pane_id: &str, harness: &str) -> AgentTarget {
    let source = PaneContext {
        workspace_id: "w1".to_owned(),
        tab_id: "w1:t1".to_owned(),
        pane_id: "w1:p1".to_owned(),
        rect: PaneRect {
            x: 20,
            y: 0,
            width: 20,
            height: 20,
        },
    };
    AgentTarget {
        provider: "herdr".to_owned(),
        protocol: 19,
        direction,
        pane_id: pane_id.to_owned(),
        workspace_id: source.workspace_id.clone(),
        tab_id: source.tab_id.clone(),
        agent_kind: HarnessKind::new(harness).expect("fixture harness"),
        agent_name: format!("{harness} {pane_id}"),
        agent_session: AgentSessionBinding::established(format!("session-{pane_id}"))
            .expect("fixture session"),
        readiness: AgentState::Idle,
        delivery: AgentDeliveryCapabilities::SUBMIT_ONLY,
        rect: PaneRect {
            x: 40,
            y: 0,
            width: 20,
            height: 20,
        },
        source,
    }
}

pub(super) fn prepare_thought(fixture: &mut Fixture) {
    let sequence = fixture.paste("exact prompt\nGrüße 第二行");
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(UiInput::Key(UiKey::Escape));
}

pub(super) fn start_submission(
    fixture: &mut Fixture,
    effects: &[Effect],
) -> proqi::ports::agent::SubmissionRequest {
    let [Effect::PrepareSubmission(attempt)] = effects else {
        panic!("expected prepared submission");
    };
    let sending = fixture.app.complete_submission_prepared(attempt.id, Ok(()));
    assert!(matches!(
        sending.as_slice(),
        [Effect::MarkSubmissionSending { .. }]
    ));
    let submitted = fixture.app.complete_submission_sending(attempt.id, Ok(()));
    let [Effect::SubmitAgent(request)] = submitted.as_slice() else {
        panic!("expected semantic submission");
    };
    request.clone()
}

pub(super) fn finish_submission(
    fixture: &mut Fixture,
    request: &proqi::ports::agent::SubmissionRequest,
    result: Result<SubmissionReceipt, AgentError>,
) -> Vec<Effect> {
    let live_before = fixture.app.state.board.live_thoughts().len();
    let journal = fixture
        .app
        .complete_submission(request.submission_id, result);
    let [Effect::FinishSubmission { removal, .. }] = journal.as_slice() else {
        panic!("expected one terminal submission journal effect");
    };
    if let Some(operation) = removal {
        assert_eq!(fixture.app.state.board.live_thoughts().len(), live_before);
        fixture
            .app
            .acknowledge_persistence(operation.sequence, true);
    }
    fixture
        .app
        .complete_submission_journaled(request.submission_id, Ok(()))
}

#[test]
fn failed_submission_preserves_thought_and_accepted_remove_is_undoable() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    let target = target(Direction::Right, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let failed = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let failed_request = start_submission(&mut fixture, &failed);
    let no_mutation = finish_submission(&mut fixture, &failed_request, Err(AgentError::TimedOut));
    assert!(no_mutation.is_empty());
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| { status.starts_with("Submission failed. Thought kept.") })
    );

    let removing = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = start_submission(&mut fixture, &removing);
    assert_eq!(request.content, "exact prompt\nGrüße 第二行");
    let completion = finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: target.clone(),
            post_state: Some(AgentState::Working),
        }),
    );
    assert!(matches!(
        completion.as_slice(),
        [Effect::StoreIntegrationContext { target: stored, .. }] if stored == &target
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());

    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn accepted_receipt_ignores_volatile_target_metadata() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    let target = target(Direction::Right, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = start_submission(&mut fixture, &effects);
    let mut revalidated = target;
    revalidated.readiness = AgentState::Blocked;
    revalidated.agent_name = "Renamed agent".to_owned();
    revalidated.rect.x = revalidated.rect.x.saturating_add(1);
    revalidated.source.rect.width = revalidated.source.rect.width.saturating_add(1);

    let completion = finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: revalidated,
            post_state: Some(AgentState::Unknown),
        }),
    );
    assert!(matches!(
        completion.as_slice(),
        [Effect::StoreIntegrationContext { .. }]
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}

#[test]
fn accepted_receipt_rejects_a_different_stable_target() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    let target = target(Direction::Right, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = start_submission(&mut fixture, &effects);
    let mut different = target;
    different.agent_session =
        AgentSessionBinding::established("different-session").expect("fixture session");

    let completion = finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: different,
            post_state: Some(AgentState::Working),
        }),
    );

    assert!(completion.is_empty());
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn multiple_targets_require_direction_and_mouse_controls_use_verified_targets() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    let up = target(Direction::Up, "w1:p2");
    let right = target(Direction::Right, "w1:p3");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![up.clone(), right.clone()]));

    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('S')))
            .is_empty()
    );
    assert_eq!(
        fixture.app.submission_mode(),
        Some(SubmissionDisposition::Keep)
    );
    let directed = fixture.effects(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    let request = start_submission(&mut fixture, &directed);
    assert_eq!(request.target, right);
    let _completed = finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: request.target.clone(),
            post_state: Some(AgentState::Working),
        }),
    );

    let _layout = fixture.app.prepare_frame(Rect::new(0, 0, 100, 10));
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('s')))
            .is_empty()
    );
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 100, 10));
    let (_, control) = layout
        .controls
        .iter()
        .find(|(target, _)| {
            *target
                == proqi::ui::HitTarget::Deliver(
                    Direction::Up,
                    SubmissionDisposition::RemoveAfterSuccess,
                )
        })
        .expect("mouse submit-and-remove control");
    let clicked = fixture.effects(UiInput::Pointer(PointerInput {
        column: control.x,
        row: control.y,
        kind: PointerKind::Down(PointerButton::Left),
        extend_selection: false,
    }));
    let clicked_request = start_submission(&mut fixture, &clicked);
    assert_eq!(clicked_request.target, up);
}

#[test]
fn all_four_verified_directions_receive_distinct_footer_targets() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    fixture.app.complete_agent_discovery(Ok(vec![
        target(Direction::Up, "w1:p2"),
        target(Direction::Right, "w1:p3"),
        target(Direction::Down, "w1:p4"),
        target(Direction::Left, "w1:p5"),
    ]));

    let terminal = draw(&mut fixture, 120, 12);
    let rendered = text(terminal.backend().buffer());
    for direction in ["↑", "→", "↓", "←"] {
        assert!(rendered.contains(direction));
    }
    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 120, 12));
    for direction in [
        Direction::Up,
        Direction::Right,
        Direction::Down,
        Direction::Left,
    ] {
        assert!(
            layout
                .controls
                .iter()
                .any(|(target, _)| *target == proqi::ui::HitTarget::Agent(direction))
        );
    }
}

#[test]
fn host_focus_refreshes_adjacent_agents() {
    let mut fixture = Fixture::new();
    assert!(matches!(
        fixture.effects(UiInput::HostFocusGained).as_slice(),
        [Effect::DiscoverAgents]
    ));
}

#[test]
fn controls_and_hint_disappear_when_discovery_is_unsupported() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target(Direction::Left, "w1:p2")]));
    let shown = draw(&mut fixture, 120, 6);
    let rendered = text(shown.backend().buffer());
    assert!(rendered.contains("← Codex"));
    assert!(rendered.contains("s Submit"));
    assert!(rendered.contains("S Submit & keep"));
    assert!(!rendered.contains("working"));
    assert!(!rendered.contains(" Send"));

    fixture
        .app
        .complete_agent_discovery(Err(AgentError::Unsupported("protocol mismatch".to_owned())));
    let hidden = fixture.app.prepare_frame(Rect::new(0, 0, 80, 6));
    assert!(
        !hidden
            .controls
            .iter()
            .any(|(target, _)| matches!(target, proqi::ui::HitTarget::Deliver(_, _)))
    );
}

#[test]
fn submission_without_a_target_refreshes_and_reports_the_verified_result() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);

    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    assert!(matches!(effects.as_slice(), [Effect::DiscoverAgents]));
    assert_eq!(fixture.app.status_text(), Some("checking adjacent agents"));

    fixture
        .app
        .complete_agent_discovery(Ok(vec![target(Direction::Left, "w1:p2")]));
    assert_eq!(fixture.app.status_text(), Some("verified 1 adjacent agent"));
    let shown = fixture.app.prepare_frame(Rect::new(0, 0, 80, 6));
    assert!(
        shown
            .controls
            .iter()
            .any(|(target, _)| matches!(target, proqi::ui::HitTarget::Deliver(_, _)))
    );
}

#[test]
fn submit_and_keep_uses_the_same_semantic_request_and_preserves_the_thought() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    let target = target(Direction::Left, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let request = start_submission(&mut fixture, &effects);
    let completion = finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target,
            post_state: Some(AgentState::Working),
        }),
    );
    assert!(
        matches!(
            completion.as_slice(),
            [Effect::StoreIntegrationContext { .. }]
        ),
        "unexpected completion: {completion:?}"
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn duplicate_submission_is_suppressed_while_the_first_attempt_is_active() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target(Direction::Left, "w1:p2")]));

    let first = fixture.effects(UiInput::Key(UiKey::Character('s')));
    assert!(matches!(first.as_slice(), [Effect::PrepareSubmission(_)]));
    assert!(
        fixture
            .effects(UiInput::Key(UiKey::Character('s')))
            .is_empty()
    );
    assert_eq!(
        fixture.app.status_text(),
        Some("this thought already has a submission in progress")
    );
}

#[test]
fn accepted_submission_removal_failure_keeps_the_exact_locked_source_for_retry() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    let target = target(Direction::Left, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = start_submission(&mut fixture, &effects);
    let journal = fixture.app.complete_submission(
        request.submission_id,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target,
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
        panic!("accepted removal must be atomic with its journal outcome");
    };
    fixture
        .app
        .acknowledge_persistence(operation.sequence, false);
    fixture.app.submission_persistence_failed(
        request.submission_id,
        &proqi::ports::store::StoreError::DiskFull,
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    assert!(
        fixture
            .app
            .state
            .thought_locked(fixture.app.state.board.live_thoughts()[0].id)
    );
    assert!(fixture.app.status_text().is_some_and(|status| {
        status.starts_with("Submission accepted, but its outcome and removal were not saved")
    }));
}

#[test]
fn in_flight_submission_locks_editing_until_the_receipt_is_journaled() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    let target = target(Direction::Left, "w1:p2");
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target.clone()]));
    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = start_submission(&mut fixture, &effects);

    fixture.input(UiInput::Key(UiKey::Enter));
    assert_eq!(
        fixture.app.status_text(),
        Some("thought has a submission in progress")
    );
    fixture.input(UiInput::Key(UiKey::Character('!')));
    assert_eq!(
        fixture.app.interaction_mode(),
        proqi::application::InteractionMode::Board
    );
    assert_eq!(
        fixture.app.state.board.live_thoughts()[0].content,
        "exact prompt\nGrüße 第二行"
    );
    let completion = finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target,
            post_state: Some(AgentState::Blocked),
        }),
    );

    assert!(matches!(
        completion.as_slice(),
        [Effect::StoreIntegrationContext { .. }]
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());
}
