use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{
        AgentSessionBinding, AgentTarget, CLAUDE_AGENT_KIND, CODEX_AGENT_KIND, HarnessKind,
        SubmissionDisposition,
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
            "kilo",
            Direction::Right,
            CursorMovement::GraphemeForward,
        ),
        (
            "kilo",
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
        assert_eq!(request.target.agent_kind.as_str(), "kilo");
    }
}

fn title(kind: &str) -> String {
    let mut characters = kind.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + characters.as_str()
    })
}
