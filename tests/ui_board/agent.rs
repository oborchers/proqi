use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{
        AgentError, AgentReadiness, AgentTarget, PaneContext, PaneRect, SubmissionReceipt,
    },
};

fn target(direction: Direction, pane_id: &str) -> AgentTarget {
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
        direction,
        pane_id: pane_id.to_owned(),
        workspace_id: source.workspace_id.clone(),
        tab_id: source.tab_id.clone(),
        agent_kind: "codex".to_owned(),
        agent_name: format!("Codex {pane_id}"),
        agent_session_id: format!("session-{pane_id}"),
        readiness: AgentReadiness::Idle,
        rect: PaneRect {
            x: 40,
            y: 0,
            width: 20,
            height: 20,
        },
        source,
    }
}

fn prepare_thought(fixture: &mut Fixture) {
    let sequence = fixture.paste("exact prompt\nGrüße 第二行");
    fixture.app.acknowledge_persistence(sequence, true);
    fixture.input(UiInput::Key(UiKey::Escape));
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
    let [Effect::SubmitAgent(failed_request)] = failed.as_slice() else {
        panic!("expected semantic submission");
    };
    let no_mutation = fixture
        .app
        .complete_submission(failed_request.submission_id, Err(AgentError::TimedOut));
    assert!(no_mutation.is_empty());
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);

    let removing = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let [Effect::SubmitAgent(request)] = removing.as_slice() else {
        panic!("expected submit-and-remove request");
    };
    assert_eq!(request.content, "exact prompt\nGrüße 第二行");
    let completion = fixture.app.complete_submission(
        request.submission_id,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: target.clone(),
            readiness: AgentReadiness::Working,
        }),
    );
    assert!(matches!(
        completion.as_slice(),
        [
            Effect::StoreIntegrationContext { target: stored, .. },
            Effect::CommitBoardOperation(operation)
        ] if stored == &target
            && operation.kind == proqi::domain::BoardOperationKind::SubmitAndRemove
    ));
    assert!(fixture.app.state.board.live_thoughts().is_empty());

    fixture.input(UiInput::Key(UiKey::Undo));
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
            .effects(UiInput::Key(UiKey::Character('s')))
            .is_empty()
    );
    assert_eq!(fixture.app.submission_mode(), Some(false));
    let directed = fixture.effects(UiInput::Key(UiKey::Move {
        movement: CursorMovement::GraphemeForward,
        extend_selection: false,
    }));
    assert!(matches!(
        directed.as_slice(),
        [Effect::SubmitAgent(request)] if request.target == right
    ));

    let layout = fixture.app.prepare_frame(Rect::new(0, 0, 100, 8));
    let (_, control) = layout
        .controls
        .iter()
        .find(|(target, _)| *target == proqi::ui::HitTarget::Submit(Direction::Up, true))
        .expect("mouse submit-and-remove control");
    let clicked = fixture.effects(UiInput::Pointer(PointerInput {
        column: control.x,
        row: control.y,
        kind: PointerKind::Down(PointerButton::Left),
    }));
    assert!(matches!(
        clicked.as_slice(),
        [Effect::SubmitAgent(request)] if request.target == up
    ));
}

#[test]
fn controls_and_hint_disappear_when_discovery_is_unsupported() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);
    fixture
        .app
        .complete_agent_discovery(Ok(vec![target(Direction::Left, "w1:p2")]));
    let shown = draw(&mut fixture, 80, 6);
    assert!(text(shown.backend().buffer()).contains("send left Codex"));

    fixture
        .app
        .complete_agent_discovery(Err(AgentError::Unsupported("protocol mismatch".to_owned())));
    let hidden = fixture.app.prepare_frame(Rect::new(0, 0, 80, 6));
    assert!(
        !hidden
            .controls
            .iter()
            .any(|(target, _)| matches!(target, proqi::ui::HitTarget::Submit(_, _)))
    );
}

#[test]
fn submission_without_a_target_refreshes_and_reports_the_verified_result() {
    let mut fixture = Fixture::new();
    prepare_thought(&mut fixture);

    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    assert!(matches!(effects.as_slice(), [Effect::DiscoverAgents]));
    assert_eq!(
        fixture.app.status.as_deref(),
        Some("checking adjacent agents")
    );

    fixture
        .app
        .complete_agent_discovery(Ok(vec![target(Direction::Left, "w1:p2")]));
    assert_eq!(
        fixture.app.status.as_deref(),
        Some("verified 1 adjacent agent")
    );
    let shown = fixture.app.prepare_frame(Rect::new(0, 0, 80, 6));
    assert!(
        shown
            .controls
            .iter()
            .any(|(target, _)| matches!(target, proqi::ui::HitTarget::Submit(_, _)))
    );
}
