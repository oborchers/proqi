use super::*;

use proqi::ports::agent::{AgentAvailability, AgentState};

#[test]
fn current_server_target_chooser_is_complete_in_a_narrow_viewport() {
    let mut fixture = Fixture::new();
    super::global_agent_delivery::prepare(&mut fixture, "focused prompt");
    let generation = super::global_agent_delivery::open(&mut fixture);
    fixture.app.complete_global_agent_discovery(
        generation,
        Ok(vec![
            super::global_agent_delivery::target(
                "w1",
                "w1:t2",
                "w1:p2",
                "Béta 世界",
                AgentState::Working,
                AgentAvailability::Available,
            ),
            super::global_agent_delivery::target(
                "w2",
                "w2:t1",
                "w2:p8",
                "Blocked receiver",
                AgentState::Blocked,
                AgentAvailability::Blocked,
            ),
        ]),
    );
    let _initial_layout = screen(&mut fixture, 64, 10);
    fixture.input(UiInput::Key(UiKey::Move {
        movement: proqi::ports::editor::CursorMovement::VisualDown,
        extend_selection: false,
    }));

    insta::assert_snapshot!(
        "global_delivery_target_chooser",
        screen(&mut fixture, 64, 10)
    );
}

#[test]
fn explicit_disposition_chooser_is_complete_in_a_shallow_viewport() {
    let mut fixture = Fixture::new();
    super::global_agent_delivery::prepare(&mut fixture, "focused prompt");
    let generation = super::global_agent_delivery::open(&mut fixture);
    fixture.app.complete_global_agent_discovery(
        generation,
        Ok(vec![super::global_agent_delivery::target(
            "w2",
            "w2:t1",
            "w2:p8",
            "Receiver",
            AgentState::Done,
            AgentAvailability::Available,
        )]),
    );
    fixture.input(UiInput::Key(UiKey::Enter));

    insta::assert_snapshot!("global_delivery_disposition", screen(&mut fixture, 50, 6));
}

fn screen(fixture: &mut Fixture, width: u16, height: u16) -> String {
    let terminal = draw_theme(fixture, width, height, ThemePreference::Dark);
    snapshot_support::snapshot_buffer(terminal.backend().buffer())
}
