use super::*;

use proqi::{domain::Direction, ports::agent::AgentError};

#[test]
fn passive_discovery_failure_clears_targets_without_interrupting_the_board() {
    let mut fixture = Fixture::new();
    fixture
        .app
        .complete_agent_discovery(Ok(vec![super::agent::target(Direction::Left, "w1:p2")]));

    fixture
        .app
        .complete_agent_discovery(Err(AgentError::Malformed(
            "directional response does not match source context".to_owned(),
        )));

    assert!(fixture.app.agent_targets().is_empty());
    assert_eq!(fixture.app.status_text(), None);
}

#[test]
fn explicit_discovery_failure_remains_visible() {
    let mut fixture = Fixture::new();
    let effects = fixture.effects(UiInput::Key(UiKey::Character('S')));
    assert!(matches!(effects.as_slice(), [Effect::DiscoverAgents]));

    fixture
        .app
        .complete_agent_discovery(Err(AgentError::Malformed(
            "directional response does not match source context".to_owned(),
        )));

    assert!(
        fixture
            .app
            .status_text()
            .is_some_and(|status| status.contains("malformed output"))
    );
}
