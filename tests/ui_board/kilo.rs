use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{
        AgentSessionBinding, AgentTarget, CLAUDE_AGENT_KIND, CODEX_AGENT_KIND, HarnessKind,
        KILO_AGENT_KIND, SubmissionDisposition, SubmissionReceipt,
    },
};

fn mixed_target(direction: Direction, pane_id: &str, kind: &str, name: &str) -> AgentTarget {
    let mut target = super::agent::target(direction, pane_id);
    target.agent_kind = HarnessKind::new(kind).expect("fixture harness kind");
    name.clone_into(&mut target.agent_name);
    target.agent_session = AgentSessionBinding::established(format!("session-{kind}-{pane_id}"))
        .expect("fixture harness session");
    target
}

#[test]
fn kilo_in_either_mixed_row_position_never_bypasses_direction_choice() {
    for (left_kind, right_kind, kilo_direction, movement) in [
        (
            CLAUDE_AGENT_KIND,
            KILO_AGENT_KIND,
            Direction::Right,
            CursorMovement::GraphemeForward,
        ),
        (
            KILO_AGENT_KIND,
            CODEX_AGENT_KIND,
            Direction::Left,
            CursorMovement::GraphemeBack,
        ),
    ] {
        let mut fixture = Fixture::new();
        super::agent::prepare_thought(&mut fixture);
        fixture.app.complete_agent_discovery(Ok(vec![
            mixed_target(Direction::Left, "w1:p2", left_kind, "left-reviewer"),
            mixed_target(Direction::Right, "w1:p3", right_kind, "right-reviewer"),
        ]));

        let terminal = draw(&mut fixture, 100, 8);
        let rendered = text(terminal.backend().buffer());
        assert!(rendered.contains(&format!("← {}", title(left_kind))));
        assert!(rendered.contains(&format!("→ {}", title(right_kind))));
        assert!(
            fixture
                .effects(UiInput::Key(UiKey::Character('s')))
                .is_empty()
        );
        assert_eq!(
            fixture.app.submission_mode(),
            Some(SubmissionDisposition::RemoveAfterSuccess)
        );
        assert!(fixture.effects(UiInput::Key(UiKey::Escape)).is_empty());
        assert_eq!(fixture.app.submission_mode(), None);
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
        assert!(
            fixture
                .effects(UiInput::Key(UiKey::Character('s')))
                .is_empty()
        );

        let effects = fixture.effects(UiInput::Key(UiKey::Move {
            movement,
            extend_selection: false,
        }));
        let request = super::agent::start_submission(&mut fixture, &effects);
        assert_eq!(request.target.direction, kilo_direction);
        assert_eq!(request.target.agent_kind.as_str(), KILO_AGENT_KIND);
    }
}

#[test]
fn kilo_receipt_with_session_upgrades_the_target_for_established_follow_up() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let mut provisional = mixed_target(Direction::Right, "w1:p2", KILO_AGENT_KIND, "kilo-reviewer");
    provisional.agent_session = AgentSessionBinding::provisional();
    fixture
        .app
        .complete_agent_discovery(Ok(vec![provisional.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let request = super::agent::start_submission(&mut fixture, &effects);
    let mut established = provisional;
    established.agent_session =
        AgentSessionBinding::established("session-kilo-established").expect("fixture Kilo session");
    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: established,
            post_state: Some(proqi::ports::agent::AgentState::Working),
        }),
    );
    assert!(matches!(
        completion.as_slice(),
        [Effect::StoreIntegrationContext { .. }]
    ));

    let follow_up = fixture.effects(UiInput::Key(UiKey::Character('S')));
    let follow_up_request = super::agent::start_submission(&mut fixture, &follow_up);
    assert_eq!(
        follow_up_request.target.agent_session.as_id(),
        Some("session-kilo-established")
    );
}

#[test]
fn provisional_kilo_receipt_removes_once_then_rediscovers_without_resending() {
    let mut fixture = Fixture::new();
    super::agent::prepare_thought(&mut fixture);
    let mut provisional = mixed_target(Direction::Right, "w1:p2", KILO_AGENT_KIND, "kilo-reviewer");
    provisional.agent_session = AgentSessionBinding::provisional();
    fixture
        .app
        .complete_agent_discovery(Ok(vec![provisional.clone()]));

    let effects = fixture.effects(UiInput::Key(UiKey::Character('s')));
    let request = super::agent::start_submission(&mut fixture, &effects);
    let completion = super::agent::finish_submission(
        &mut fixture,
        &request,
        Ok(SubmissionReceipt {
            submission_id: request.submission_id,
            target: provisional,
            post_state: Some(proqi::ports::agent::AgentState::Working),
        }),
    );

    assert_eq!(
        completion
            .iter()
            .filter(|effect| matches!(effect, Effect::DiscoverAgents))
            .count(),
        1
    );
    assert!(
        !completion
            .iter()
            .any(|effect| matches!(effect, Effect::SubmitAgent(_)))
    );
    assert!(fixture.app.state.board.live_thoughts().is_empty());
    fixture.input(UiInput::Key(UiKey::Escape));
    fixture.input(UiInput::Key(UiKey::Undo));
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

fn title(kind: &str) -> String {
    let mut characters = kind.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}
