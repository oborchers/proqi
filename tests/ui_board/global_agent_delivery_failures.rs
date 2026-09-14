use super::*;

use proqi::ports::{
    agent::{AgentAvailability, AgentError, AgentState},
    attachment_accessibility::AttachmentAccessFailure,
};

#[test]
fn rapid_activation_reports_loading_and_discovery_failure_truthfully() {
    let mut fixture = Fixture::new();
    super::global_agent_delivery::prepare(&mut fixture, "keep me");
    let generation = super::global_agent_delivery::open(&mut fixture);

    assert!(fixture.effects(crate::key_input(UiKey::Enter)).is_empty());
    assert_eq!(
        fixture.app.status_text(),
        Some("agent discovery is still in progress")
    );

    fixture.app.complete_global_agent_discovery(
        generation,
        Err(AgentError::Unavailable("synthetic failure".to_owned())),
    );
    assert!(fixture.effects(crate::key_input(UiKey::Enter)).is_empty());
    assert_eq!(
        fixture.app.status_text(),
        Some("agent discovery failed; refresh and try again")
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
}

#[test]
fn cancellation_failures_and_repeated_activation_preserve_sources_without_resubmission() {
    let mut cancelled = Fixture::new();
    super::global_agent_delivery::prepare(&mut cancelled, "cancelled");
    let generation = super::global_agent_delivery::open(&mut cancelled);
    cancelled
        .app
        .complete_global_agent_discovery(generation, Ok(vec![receiver()]));
    assert!(
        cancelled
            .effects(crate::key_input(UiKey::Escape))
            .is_empty()
    );
    assert_eq!(cancelled.app.state.board.live_thoughts().len(), 1);

    for error in [
        AgentError::TimedOut,
        AgentError::Rejected {
            code: "busy".to_owned(),
            message: "receiver rejected".to_owned(),
        },
    ] {
        let mut fixture = Fixture::new();
        super::global_agent_delivery::prepare(&mut fixture, "failure source");
        let destination = receiver();
        let generation = super::global_agent_delivery::open(&mut fixture);
        fixture
            .app
            .complete_global_agent_discovery(generation, Ok(vec![destination.clone()]));
        fixture.input(crate::key_input(UiKey::Enter));
        let prepared = fixture.effects(crate::key_input(UiKey::Enter));
        let request = super::agent::start_submission(&mut fixture, &prepared);

        fixture.input(crate::key_input(UiKey::Character(':')));
        for character in "submit to agent".chars() {
            fixture.input(crate::key_input(UiKey::Character(character)));
        }
        let layout = fixture.app.prepare_frame(Rect::new(0, 0, 72, 10));
        assert_eq!(layout.overlay.expect("Commands").item_interactive, [false]);
        assert!(fixture.effects(crate::key_input(UiKey::Enter)).is_empty());
        assert!(
            text(draw(&mut fixture, 72, 10).backend().buffer())
                .contains("Thought has an operation in progress")
        );
        fixture.input(crate::key_input(UiKey::Escape));

        super::agent::finish_submission(&mut fixture, &request, Err(error));
        assert_eq!(fixture.app.state.board.live_thoughts().len(), 1);
    }
}

#[test]
fn global_delivery_uses_the_shared_fresh_attachment_preflight() {
    let mut fixture = super::attachment_accessibility::submission_fixture();
    fixture.input(crate::key_input(UiKey::Character('k')));
    let generation = super::global_agent_delivery::open(&mut fixture);
    fixture
        .app
        .complete_global_agent_discovery(generation, Ok(vec![receiver()]));
    fixture.input(crate::key_input(UiKey::Enter));
    let effects = fixture.effects(crate::key_input(UiKey::Enter));
    let preflight = super::attachment_accessibility::attachment_batch(&effects);
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, Effect::PrepareSubmission(_)))
    );
    let completion = super::attachment_accessibility::complete(
        preflight,
        Err(AttachmentAccessFailure::TimedOut),
    );
    assert!(
        fixture
            .app
            .complete_attachment_checks(completion)
            .is_empty()
    );
    assert_eq!(
        fixture.app.status_text(),
        Some("Proqi cannot access 1 attachment")
    );
    assert_eq!(fixture.app.state.board.live_thoughts().len(), 2);
}

fn receiver() -> proqi::ports::agent::AgentTarget {
    super::global_agent_delivery::target(
        "w2",
        "w2:t1",
        "w2:p8",
        "Receiver",
        AgentState::Idle,
        AgentAvailability::Available,
    )
}
