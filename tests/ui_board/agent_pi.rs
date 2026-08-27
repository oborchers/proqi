use super::*;

use proqi::{
    domain::Direction,
    ports::agent::{CLAUDE_AGENT_KIND, CODEX_AGENT_KIND, HarnessKind},
};

fn mixed_target(
    direction: Direction,
    pane_id: &str,
    kind: &str,
) -> proqi::ports::agent::AgentTarget {
    let mut target = super::agent::target(direction, pane_id);
    target.agent_kind = HarnessKind::new(kind).expect("fixture harness");
    target.agent_name = format!("{kind}-review");
    target
}

#[test]
fn pi_in_both_mixed_row_positions_never_bypasses_direction_choice() {
    for (left, right, choice, expected) in [
        (CLAUDE_AGENT_KIND, "pi", 'h', CLAUDE_AGENT_KIND),
        (CLAUDE_AGENT_KIND, "pi", 'l', "pi"),
        ("pi", CODEX_AGENT_KIND, 'h', "pi"),
        ("pi", CODEX_AGENT_KIND, 'l', CODEX_AGENT_KIND),
    ] {
        let mut fixture = Fixture::new();
        super::agent::prepare_thought(&mut fixture);
        fixture.app.complete_agent_discovery(Ok(vec![
            mixed_target(Direction::Left, "w1:p2", left),
            mixed_target(Direction::Right, "w1:p3", right),
        ]));

        assert!(
            fixture
                .effects(UiInput::Key(UiKey::Character('s')))
                .is_empty()
        );
        let effects = fixture.effects(UiInput::Key(UiKey::Character(choice)));
        let request = super::agent::start_submission(&mut fixture, &effects);
        assert_eq!(request.target.agent_kind.as_str(), expected);
    }
}

#[test]
fn mixed_pi_rows_render_both_verified_harness_labels_and_directions() {
    for (left, right, expected) in [
        (CLAUDE_AGENT_KIND, "pi", ["← Claude", "→ Pi"]),
        ("pi", CODEX_AGENT_KIND, ["← Pi", "→ Codex"]),
    ] {
        let mut fixture = Fixture::new();
        super::agent::prepare_thought(&mut fixture);
        fixture.app.complete_agent_discovery(Ok(vec![
            mixed_target(Direction::Left, "w1:p2", left),
            mixed_target(Direction::Right, "w1:p3", right),
        ]));

        let terminal = draw(&mut fixture, 100, 8);
        let rendered = text(terminal.backend().buffer());
        for label in expected {
            assert!(rendered.contains(label), "missing {label}: {rendered}");
        }
    }
}
