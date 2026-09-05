use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{AgentSessionBinding, AgentTarget, HarnessKind},
};

fn harness_target(direction: Direction, pane_id: &str, kind: &str) -> AgentTarget {
    let mut target = super::agent::target(direction, pane_id)
        .with_agent_kind(HarnessKind::new(kind).expect("fixture harness"));
    target.agent_name = format!("{kind} qualifier");
    target.with_agent_session(
        AgentSessionBinding::established(format!("{kind}-session")).expect("fixture session"),
    )
}

#[test]
fn hermes_in_both_mixed_row_positions_never_bypasses_direction_choice() {
    for (left_kind, right_kind) in [("claude", "hermes"), ("hermes", "codex")] {
        for (key, expected_direction, expected_kind) in [
            ('h', Direction::Left, left_kind),
            ('l', Direction::Right, right_kind),
        ] {
            let mut fixture = Fixture::new();
            super::agent::prepare_thought(&mut fixture);
            let left = harness_target(Direction::Left, "w1:p2", left_kind);
            let right = harness_target(Direction::Right, "w1:p3", right_kind);
            fixture.app.complete_agent_discovery(Ok(vec![left, right]));

            let rendered = text(draw(&mut fixture, 100, 9).backend().buffer());
            assert!(rendered.contains(&format!("← {}", title(left_kind))));
            assert!(rendered.contains(&format!("→ {}", title(right_kind))));
            assert!(
                fixture
                    .effects(UiInput::Key(UiKey::Character('s')))
                    .is_empty()
            );
            let effects = fixture.effects(UiInput::Key(UiKey::Character(key)));
            let request = super::agent::start_submission(&mut fixture, &effects);
            assert_eq!(
                request.target.adjacent_direction(),
                Some(expected_direction)
            );
            assert_eq!(request.target.agent_kind().as_str(), expected_kind);
        }
    }
}

fn title(kind: &str) -> String {
    let mut characters = kind.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default()
}
